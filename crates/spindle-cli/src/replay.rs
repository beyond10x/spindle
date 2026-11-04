//! `run --scenario`: replaying an authored `ess-scenario` document against every component of a
//! system in one process.
//!
//! The document is read and checked against the model by ESS's own reader
//! ([`ess_conformance::authored::compile`]), and its compiled steps are executed by ESS's own
//! runner ([`ess_conformance::Runner`]) against [`HostTarget`]: a [`Host`] with fresh in-memory
//! stores for every replay, whatever `store.durable` is bound to, so a replay writes nothing to
//! disk. Spindle compares nothing itself; what holds is the runner's answer.
//!
//! The runner reports a scenario as a whole and goes on past a failed expectation. To name the
//! first step that diverges, the compiled steps are cut into the document's own steps (what is
//! arranged, each timeline act, each assertion). The whole scenario is replayed first; when it
//! does not pass, the first divergent step is found by bisection over prefixes, each replayed
//! from an empty system: a prefix that does not pass stays failing when it grows, so about
//! `log2(steps)` replays find the shortest one. The divergence reported is the replay of that
//! shortest prefix, which runs no step after the divergent one.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use ess_compiler::EssIr;
use ess_compiler::refs::{ErrorRef, EventRef, OutcomeRef};
use ess_conformance::authored::{self, Source};
use ess_conformance::{
    AdmittedSuite, AdvancingClock, ConformanceScenario, ConformanceSuite, ConformanceTarget,
    DeclaredErrorValue, EventObservationRequest, ExternalOutcomeControl, Ids,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, Runner, RunnerConfig,
    ScenarioContext, ScenarioId, ScenarioResult, ScenarioStep, SemanticCommandRequest,
    SemanticCommandResult, SemanticViewRequest, SemanticViewResult, Status, SuiteProvenance,
    TargetError,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::ConsistencyToken;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, SystemClock};
use spindle_kernel::ExecuteError;

/// Exit status of a replay the environment refused.
const REFUSED: u8 = 3;

/// The most document steps a replay takes; a longer document is refused before any replay.
pub(crate) const MOST_STEPS: usize = 1000;

/// The store implementation every replay keeps its state in.
const MEMORY: &str = "memory";

/// Replays the scenario in `scenario` against `spec` admitted on `environment`.
///
/// Exits 0 when every step holds, 1 at the first step that does not, when ESS refuses the
/// document or when it has more than [`MOST_STEPS`] steps, and 3, running nothing, when admission
/// is refused.
pub(crate) fn replay(spec: &Path, environment: &Path, scenario: &Path) -> Result<ExitCode> {
    let loaded = spindle_spec::load(spec)?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    let document = std::fs::read_to_string(scenario)
        .with_context(|| format!("reading {}", scenario.display()))?;

    let authoring = authored::compile(
        &loaded.ir,
        &[Source::new(scenario.display().to_string(), document)],
    );
    if !authoring.is_complete() {
        for refusal in &authoring.refusals {
            eprintln!("{refusal}");
        }
        return Ok(ExitCode::FAILURE);
    }
    let mut compiled = authoring.scenarios.into_iter();
    let (Some((id, compiled)), None) = (compiled.next(), compiled.next()) else {
        bail!("{} compiled to no single scenario", scenario.display());
    };
    let steps = steps(&compiled.steps);
    if steps.len() > MOST_STEPS {
        eprintln!(
            "b10x-spindle: {} has {} steps; a replay takes at most {MOST_STEPS}",
            scenario.display(),
            steps.len()
        );
        return Ok(ExitCode::FAILURE);
    }

    if let Err(refusal) = spindle_host::admit(&loaded.ir, &parsed) {
        println!("{refusal}");
        return Ok(ExitCode::from(REFUSED));
    }
    let memory = in_memory(&parsed);
    match Host::new(
        loaded.ir.clone(),
        &memory,
        Arc::new(SystemClock),
        Path::new(""),
    ) {
        Ok(_) => {}
        Err(HostError::Refused(refusal)) => {
            println!("{refusal}");
            return Ok(ExitCode::from(REFUSED));
        }
        Err(error) => return Err(error.into()),
    }

    let replays = Replays {
        ir: &loaded.ir,
        environment: &memory,
        id: &id,
        scenario: &compiled,
        origin: scenario,
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let Some(last) = steps.last() else {
        return Ok(ExitCode::SUCCESS);
    };
    let whole = replays.run(last.end)?;
    if whole.result.status == Status::Passed {
        for step in &steps {
            writeln!(out, "{}: holds", step.label)?;
        }
        out.flush()?;
        return Ok(ExitCode::SUCCESS);
    }
    // ESS admits no prefix that arranges state and then claims nothing, so the arrangement is
    // replayed with the first act; the target says whether it was the arrangement that failed.
    let arranged = steps.len() > 1 && steps[0].label == ARRANGE;
    let (mut passing, mut failing, mut divergent) = (usize::from(arranged), steps.len(), whole);
    while failing - passing > 1 {
        let middle = passing + (failing - passing) / 2;
        let replayed = replays.run(steps[middle - 1].end)?;
        if replayed.result.status == Status::Passed {
            passing = middle;
        } else {
            failing = middle;
            divergent = replayed;
        }
    }
    let index = if arranged && failing == 2 && divergent.unarranged {
        0
    } else {
        failing - 1
    };
    for step in &steps[..index] {
        writeln!(out, "{}: holds", step.label)?;
    }
    writeln!(out, "{}: diverges", steps[index].label)?;
    for check in divergent
        .result
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
    writeln!(
        out,
        "  this replay ran {} command(s), none after {}",
        divergent.commands, steps[index].label
    )?;
    out.flush()?;
    Ok(ExitCode::FAILURE)
}

/// `environment` with every store capability it provides bound to memory only.
fn in_memory(environment: &Environment) -> Environment {
    let mut memory = environment.clone();
    for (capability, implementations) in &mut memory.provides {
        if capability.starts_with("store.") {
            *implementations = vec![MEMORY.to_owned()];
        }
    }
    memory
}

/// What replaying one prefix needs.
struct Replays<'a> {
    ir: &'a EssIr,
    environment: &'a Environment,
    id: &'a ScenarioId,
    scenario: &'a ConformanceScenario,
    origin: &'a Path,
}

/// One replay: the runner's answer and how many commands it sent to the host.
struct Replayed {
    result: ScenarioResult,
    commands: usize,
    /// Whether it stopped at state the target could not arrange.
    unarranged: bool,
}

impl Replays<'_> {
    /// Replays the first `end` compiled steps from an empty system.
    fn run(&self, end: usize) -> Result<Replayed> {
        let prefix = ConformanceScenario {
            steps: self.scenario.steps[..end].to_vec(),
            ..self.scenario.clone()
        };
        let mut suite = ConformanceSuite::new(SuiteProvenance::of(self.ir));
        if let Err(id) = suite.insert(self.id.clone(), prefix) {
            bail!("`{id}` is already in the suite");
        }
        suite.select_fresh_format_for(self.ir);
        let admitted = AdmittedSuite::from_suite(&suite)
            .with_context(|| format!("admitting {}", self.origin.display()))?;
        let target = HostTarget::new(self.ir.clone(), self.environment.clone());
        let report = Runner::new(
            RunnerConfig::default(),
            AdvancingClock::default(),
            Ids::for_suite(&suite),
        )
        .run_admitted(&admitted, &target)
        .into_report();
        let result = report
            .scenarios
            .into_iter()
            .next()
            .with_context(|| format!("the runner reported nothing for `{}`", self.id))?;
        Ok(Replayed {
            result,
            commands: target.commands.get(),
            unarranged: target.unarranged.get(),
        })
    }
}

/// One step of the document: where its compiled steps end, and what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    end: usize,
    label: String,
}

/// The label of what a document arranges before its first act.
const ARRANGE: &str = "arrange";

/// The document's steps over its compiled steps.
///
/// A timeline act compiles to one step that sends or awaits its command, and an assertion to one
/// step that reads its view; each such step opens a document step, which runs until the next one
/// opens. What the compiler places immediately before a send for it (the external answer, a held
/// `now`, the windows it measures) belongs to that send. Whatever precedes the first act
/// (fixtures, arranged state) is a step of its own, `arrange`.
fn steps(compiled: &[ScenarioStep]) -> Vec<Step> {
    let (mut acts, mut asserts) = (0_usize, 0_usize);
    let mut opened: Vec<(usize, String)> = Vec::new();
    for (index, step) in compiled.iter().enumerate() {
        let label = match step {
            ScenarioStep::ExecuteCommand { command, .. }
            | ScenarioStep::ExecuteCommandWithoutInput { command, .. }
            | ScenarioStep::AwaitOutcome { command, .. }
            | ScenarioStep::ExpectNotIssued { command, .. } => {
                acts += 1;
                format!("timeline[{}] {command}", acts - 1)
            }
            ScenarioStep::QueryView { view, .. }
            | ScenarioStep::EventuallyView { view, .. }
            | ScenarioStep::ExpectHalt { view, .. }
            | ScenarioStep::EventuallyHalt { view, .. } => {
                asserts += 1;
                format!("assert[{}] {view}", asserts - 1)
            }
            _ => continue,
        };
        let floor = opened.last().map_or(0, |(start, _)| start + 1);
        let mut start = index;
        while start > floor
            && matches!(
                compiled[start - 1],
                ScenarioStep::ConfigureExternalOutcome { .. }
                    | ScenarioStep::HoldNow { .. }
                    | ScenarioStep::ExpectNotBefore { .. }
                    | ScenarioStep::ExpectWithin { .. }
                    | ScenarioStep::ExpectQuiet { .. }
            )
        {
            start -= 1;
        }
        if opened.is_empty() && start > 0 {
            opened.push((0, ARRANGE.to_owned()));
        }
        opened.push((start, label));
    }
    if opened.is_empty() {
        return vec![Step {
            end: compiled.len(),
            label: ARRANGE.to_owned(),
        }];
    }
    let ends = opened
        .iter()
        .skip(1)
        .map(|(start, _)| *start)
        .chain([compiled.len()])
        .collect::<Vec<_>>();
    opened
        .into_iter()
        .zip(ends)
        .map(|((_, label), end)| Step { end, label })
        .collect()
}

/// Every component of the system in one [`Host`], opened afresh for each scenario.
struct HostTarget {
    ir: EssIr,
    environment: Environment,
    running: RefCell<Option<Running>>,
    /// How many commands this target has sent to a host.
    commands: Cell<usize>,
    /// Whether the runner asked for state the target does not arrange.
    unarranged: Cell<bool>,
}

/// The host of the scenario in progress, and every event its commands published.
struct Running {
    host: Host,
    /// How many commands have run: the consistency token of the last.
    writes: u64,
    events: Vec<ObservedEvent>,
}

impl HostTarget {
    fn new(ir: EssIr, environment: Environment) -> Self {
        Self {
            ir,
            environment,
            running: RefCell::new(None),
            commands: Cell::new(0),
            unarranged: Cell::new(false),
        }
    }
}

impl ConformanceTarget for HostTarget {
    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        Ok(ImplementationIdentity::new(
            "b10x-spindle",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    fn fixture_values(
        &self,
        _scenario: &ScenarioContext,
        _contract: &ess_conformance::fixtures::Contract,
    ) -> Result<BTreeMap<String, Node>, TargetError> {
        self.unarranged.set(true);
        Err(TargetError::unsupported(
            "fixture values",
            "the in-process host provides no fixture",
        ))
    }

    fn establish_entity(
        &self,
        request: ess_conformance::target::EntitySetupRequest,
    ) -> Result<(), TargetError> {
        self.unarranged.set(true);
        Err(TargetError::unsupported(
            format!("establishing `{}`", request.entity),
            "the in-process host holds only state its commands wrote",
        ))
    }

    fn begin_scenario(&self, _scenario: &ScenarioContext) -> Result<(), TargetError> {
        let host = Host::new(
            self.ir.clone(),
            &self.environment,
            Arc::new(SystemClock),
            Path::new(""),
        )
        .map_err(|error| TargetError::unavailable("starting the host", error.to_string()))?;
        *self.running.borrow_mut() = Some(Running {
            host,
            writes: 0,
            events: Vec::new(),
        });
        Ok(())
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        let command = request.command.to_string();
        if request.actor.is_some() || request.caller.is_some() {
            return Err(TargetError::unsupported(
                format!("`{command}` sent as an actor or a caller"),
                "the in-process host checks no grant",
            ));
        }
        let input = request
            .input
            .iter()
            .map(|(field, value)| {
                serde_json::to_value(value)
                    .map(|value| (field.clone(), value))
                    .map_err(|error| {
                        TargetError::unavailable(format!("sending `{field}`"), error.to_string())
                    })
            })
            .collect::<Result<Map<String, Value>, _>>()?;
        let mut running = self.running.borrow_mut();
        let running = running
            .as_mut()
            .ok_or_else(|| TargetError::unavailable(&command, "no scenario is open"))?;
        self.commands.set(self.commands.get() + 1);
        running.writes += 1;
        let written = written(running.writes)?;
        match running.host.submit(&command, input) {
            Ok(report) => {
                let submitted = report
                    .ran
                    .first()
                    .ok_or_else(|| TargetError::unavailable(&command, "the host ran nothing"))?;
                let mut result = SemanticCommandResult::took(outcome(
                    &request.command,
                    &submitted.executed.outcome,
                )?)
                .with_consistency(written);
                for event in &submitted.executed.events {
                    result = result.emitting(observed(event, &request)?);
                }
                for ran in &report.ran {
                    for event in &ran.executed.events {
                        running.events.push(observed(event, &request)?);
                    }
                }
                Ok(result)
            }
            Err(HostError::CommandRefused {
                outcome: taken,
                error,
                ..
            }) => {
                let reference = error.name.parse::<ErrorRef>().map_err(|parse| {
                    TargetError::unavailable(&command, format!("error `{}`: {parse}", error.name))
                })?;
                let mut declared = DeclaredErrorValue::new(reference);
                for (field, value) in &error.fields {
                    declared = declared.with(field.clone(), node(value)?);
                }
                Ok(
                    SemanticCommandResult::took(outcome(&request.command, &taken)?)
                        .with_error(declared)
                        .with_consistency(written),
                )
            }
            Err(HostError::Execute { error, .. }) => match *error {
                ExecuteError::Undeclared { .. } => Ok(SemanticCommandResult::undeclared()),
                ExecuteError::Unsupported { construct } => Err(TargetError::unsupported(
                    format!("running `{command}`"),
                    construct,
                )),
                other => Err(TargetError::unavailable(
                    format!("running `{command}`"),
                    other.to_string(),
                )),
            },
            Err(HostError::Unsupported(construct)) => Err(TargetError::unsupported(
                format!("running `{command}`"),
                construct,
            )),
            Err(other) => Err(TargetError::unavailable(
                format!("running `{command}`"),
                other.to_string(),
            )),
        }
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        let view = request.view.to_string();
        if !request.params.is_empty() {
            return Err(TargetError::unsupported(
                format!("`{view}` read with parameters"),
                "the in-process host lists views without parameters only",
            ));
        }
        let mut running = self.running.borrow_mut();
        let running = running
            .as_mut()
            .ok_or_else(|| TargetError::unavailable(&view, "no scenario is open"))?;
        let rows = match running.host.list(&view) {
            Ok(rows) => rows,
            Err(HostError::Unsupported(construct)) => {
                return Err(TargetError::unsupported(
                    format!("reading `{view}`"),
                    construct,
                ));
            }
            Err(other) => {
                return Err(TargetError::unavailable(
                    format!("reading `{view}`"),
                    other.to_string(),
                ));
            }
        };
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|(field, value)| node(value).map(|value| (field.clone(), value)))
                    .collect::<Result<BTreeMap<_, _>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()
            .map(SemanticViewResult::of)
    }

    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        Ok(self
            .running
            .borrow()
            .as_ref()
            .map(|running| {
                running
                    .events
                    .iter()
                    .filter(|event| {
                        event.event == request.event
                            && event.correlation.as_ref() == Some(&request.correlation)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        Err(TargetError::unsupported(
            format!("forcing `{}`", request.force),
            "the in-process host answers no external cause",
        ))
    }

    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        Err(TargetError::unsupported(
            format!("redelivering `{}`", request.event),
            "the in-process host delivers each event once",
        ))
    }

    fn end_scenario(&self, _scenario: &ScenarioContext) -> Result<(), TargetError> {
        if let Some(mut running) = self.running.borrow_mut().take() {
            running.host.stop();
        }
        Ok(())
    }
}

/// The token of the `count`th command run.
///
/// Every command's change and everything its events lead to is committed before
/// [`Host::submit`] returns, and every view is read from those same stores, so any read made after
/// a command is at least as new as it: the token only names the write.
fn written(count: u64) -> Result<ConsistencyToken, TargetError> {
    ConsistencyToken::new(format!("write-{count}"))
        .map_err(|error| TargetError::unavailable("naming a write", error.to_string()))
}

/// The outcome `taken` of `command`, as the suite names it.
fn outcome(
    command: &ess_compiler::refs::CommandRef,
    taken: &str,
) -> Result<OutcomeRef, TargetError> {
    OutcomeName::new(taken)
        .map(|name| OutcomeRef::new(command.clone(), name))
        .map_err(|error| {
            TargetError::unavailable(format!("`{command}` took `{taken}`"), error.to_string())
        })
}

/// One event a command published, in the scenario the request belongs to.
fn observed(
    event: &spindle_kernel::Event,
    request: &SemanticCommandRequest,
) -> Result<ObservedEvent, TargetError> {
    let reference = event.name.parse::<EventRef>().map_err(|error| {
        TargetError::unavailable(format!("publishing `{}`", event.name), error.to_string())
    })?;
    let mut observed = ObservedEvent::new(reference).in_activity(request.correlation.clone());
    for (field, value) in &event.payload {
        observed = observed.with(field.clone(), node(value)?);
    }
    Ok(observed)
}

/// `value` as ESS's value, read exactly: an integer as the integer.
fn node(value: &Value) -> Result<Node, TargetError> {
    serde_json::from_value::<Node>(value.clone())
        .map_err(|error| TargetError::unsupported(format!("the value {value}"), error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(json: &str) -> ScenarioStep {
        serde_json::from_str(json).expect("a compiled step")
    }

    #[test]
    fn an_external_answer_belongs_to_the_send_it_precedes() {
        let compiled = [
            step(r#"{"step":"execute_command","command":"a.b.Open","input":{}}"#),
            step(r#"{"step":"expect_no_error"}"#),
            step(
                r#"{"step":"configure_external_outcome","force":{"command":"a.b.Close","outcome":"vetoed"}}"#,
            ),
            step(r#"{"step":"execute_command","command":"a.b.Close","input":{}}"#),
            step(r#"{"step":"query_view","view":"a.b.All"}"#),
            step(r#"{"step":"expect_no_events"}"#),
        ];
        assert_eq!(
            steps(&compiled),
            [
                Step {
                    end: 2,
                    label: "timeline[0] a.b.Open".to_owned()
                },
                Step {
                    end: 4,
                    label: "timeline[1] a.b.Close".to_owned()
                },
                Step {
                    end: 6,
                    label: "assert[0] a.b.All".to_owned()
                },
            ]
        );
    }

    #[test]
    fn what_precedes_the_first_act_is_arranged_and_a_window_belongs_to_the_act_it_measures() {
        let compiled = [
            step(r#"{"step":"expect_no_events"}"#),
            step(r#"{"step":"execute_command","command":"a.b.Open","input":{}}"#),
            step(r#"{"step":"mark_instant","instant":"opened"}"#),
            step(r#"{"step":"expect_within","instant":"opened","elapsed":20}"#),
            step(r#"{"step":"execute_command","command":"a.b.Close","input":{}}"#),
        ];
        let labelled = steps(&compiled)
            .into_iter()
            .map(|step| (step.end, step.label))
            .collect::<Vec<_>>();
        assert_eq!(
            labelled,
            [
                (1, ARRANGE.to_owned()),
                (3, "timeline[0] a.b.Open".to_owned()),
                (5, "timeline[1] a.b.Close".to_owned()),
            ]
        );
    }
}
