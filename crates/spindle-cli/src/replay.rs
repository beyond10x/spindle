//! `run --scenario`: replaying an authored `ess-scenario` document against every component of a
//! system in one process. A specification that declares no component is loaded with one implied
//! component that owns every domain ([`spindle_spec::load_with_implied_component`]), so the host
//! routes every command to it and admission derives what it requires.
//!
//! The document is read and checked against the model by ESS's own reader
//! ([`ess_conformance::authored::compile`]), and its compiled steps are executed by ESS's own
//! runner ([`ess_conformance::Runner`]) against [`HostTarget`]: a host with fresh in-memory
//! stores for every replay, whatever `store.durable` is bound to, so a replay writes nothing to
//! disk. Spindle compares nothing itself; what holds is the runner's answer. Each act's line also
//! names what its command answered ([`HostTarget::answered`]): an act that states no outcome
//! compiles to no outcome check, so the runner passes it refused, and the line says so. An
//! external cause the scenario does not force is answered by `--decisions`
//! ([`HostTarget::answering`]).
//!
//! The runner reports a scenario as a whole and goes on past a failed expectation. To name the
//! first step that diverges, the compiled steps are cut into the document's own steps (what is
//! arranged, each timeline act, each assertion). The whole scenario is replayed first; when it
//! does not pass, the first divergent step is found by bisection over prefixes, each replayed
//! from an empty system: a prefix that does not pass stays failing when it grows, so about
//! `log2(steps)` replays find the shortest one. The divergence reported is the replay of that
//! shortest prefix, which runs no step after the divergent one. A prefix keeps the scenario's
//! one-time trace only as far as its own steps reach ([`prefix`]), so ESS admits it.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use ess_compiler::EssIr;
use ess_conformance::authored::{self, Source};
use ess_conformance::one_time_response::Trace;
use ess_conformance::{
    AdmittedSuite, AdvancingClock, ConformanceScenario, ConformanceSuite, Ids, Runner,
    RunnerConfig, ScenarioId, ScenarioResult, ScenarioStep, Status, SuiteProvenance,
};
use spindle_capability::{Admission, Environment};
use spindle_conformance::{Answered, HostTarget, in_memory};
use spindle_host::{Answers, Clock, HostError, ManualClock, Refusal, SystemClock};

/// Exit status of a replay the environment refused.
const REFUSED: u8 = 3;

/// The most document steps a replay takes; a longer document is refused before any replay.
pub(crate) const MOST_STEPS: usize = 1000;

/// Replays the scenario in `scenario` against `spec` admitted on `environment`.
///
/// An external cause the scenario does not force is answered by `decisions`, a decisions file
/// ([`crate::decisions::load`]); one neither forces nor answers does not hold.
///
/// Exits 0 when every step holds, 1 at the first step that does not, when ESS refuses the
/// document, when it has more than [`MOST_STEPS`] steps or when the decisions file is refused,
/// and 3, running nothing, when admission is refused.
pub(crate) fn replay(
    spec: &Path,
    environment: &Path,
    scenario: &Path,
    decisions: Option<&Path>,
    clock: Option<SystemTime>,
    mint: Option<u64>,
) -> Result<ExitCode> {
    let loaded = spindle_spec::load_with_implied_component(spec)?;
    let answers = decisions
        .map(|path| {
            crate::decisions::load(&loaded.ir, Some(path), false).map(|run| run.answers().clone())
        })
        .transpose()?;
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
        return refused(spec, &refusal);
    }
    let memory = in_memory(&parsed);
    match HostTarget::new(loaded.ir.clone(), memory.clone()).open() {
        Ok(_) => {}
        Err(HostError::Refused(refusal)) => return refused(spec, &refusal),
        Err(error) => return Err(error.into()),
    }

    let replays = Replays {
        ir: &loaded.ir,
        environment: &memory,
        id: &id,
        scenario: &compiled,
        origin: scenario,
        answers: answers.as_ref(),
        clock,
        mint,
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let Some(last) = steps.last() else {
        return Ok(ExitCode::SUCCESS);
    };
    let sends = sends(&compiled.steps, &steps);
    let whole = replays.run(last.end)?;
    if whole.result.status == Status::Passed {
        for (step, send) in steps.iter().zip(&sends) {
            writeln!(out, "{}: holds{}", step.label, whole.answer(*send))?;
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
    for (step, send) in steps[..index].iter().zip(&sends) {
        writeln!(out, "{}: holds{}", step.label, divergent.answer(*send))?;
    }
    writeln!(
        out,
        "{}: diverges{}",
        steps[index].label,
        divergent.answer(sends[index])
    )?;
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

/// Prints `refusal` of the specification at `spec` and answers [`REFUSED`].
///
/// A specification that declares no component was refused as its implied component
/// ([`spindle_spec::load_with_implied_component`]), a component it never declared: it is refused
/// in the words stdin `run` uses, every capability it lacks and no component.
fn refused(spec: &Path, refusal: &Refusal) -> Result<ExitCode> {
    if spindle_spec::load_model(spec)?.ir.components().is_empty() {
        let missing = refusal
            .missing
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        println!("{}", serde_json::to_value(Admission::Refused { missing })?);
    } else {
        println!("{refusal}");
    }
    Ok(ExitCode::from(REFUSED))
}

/// What replaying one prefix needs.
struct Replays<'a> {
    ir: &'a EssIr,
    environment: &'a Environment,
    id: &'a ScenarioId,
    scenario: &'a ConformanceScenario,
    origin: &'a Path,
    /// `--decisions`: each replay's host answers a cause the scenario does not force from it.
    answers: Option<&'a Answers>,
    /// `--clock`: each replay's host decides `now` on a manual clock standing at this instant.
    clock: Option<SystemTime>,
    /// `--mint-seed`: each replay's host mints from this seed.
    mint: Option<u64>,
}

/// One replay: the runner's answer and how many commands it sent to the host.
struct Replayed {
    result: ScenarioResult,
    commands: usize,
    /// What each command the runner sent answered, in the order sent.
    answered: Vec<Answered>,
    /// Whether it stopped at state the target could not arrange.
    unarranged: bool,
}

impl Replayed {
    /// What the `send`-th command of this replay answered, as its act's line ends: the outcome
    /// taken and the declared error it carries, `undeclared` or `not granted`; nothing for a step
    /// that sends no command or a command the target failed to run.
    fn answer(&self, send: Option<usize>) -> String {
        match send.and_then(|send| self.answered.get(send)) {
            Some(Answered::Took {
                outcome,
                error: None,
            }) => format!(", outcome {outcome}"),
            Some(Answered::Took {
                outcome,
                error: Some(error),
            }) => format!(", outcome {outcome}, error {error}"),
            Some(Answered::Undeclared) => ", undeclared".to_owned(),
            Some(Answered::NotGranted) => ", not granted".to_owned(),
            Some(Answered::Failed) | None => String::new(),
        }
    }
}

impl Replays<'_> {
    /// Replays the first `end` compiled steps from an empty system.
    fn run(&self, end: usize) -> Result<Replayed> {
        let suite = suite(self.ir, self.id, prefix(self.scenario, end))?;
        let admitted = AdmittedSuite::from_suite(&suite)
            .with_context(|| format!("admitting {}", self.origin.display()))?;
        let clock: Arc<dyn Clock> = match self.clock {
            Some(at) => Arc::new(ManualClock::at(at)),
            None => Arc::new(SystemClock),
        };
        let target =
            HostTarget::clocked(self.ir.clone(), self.environment.clone(), clock, self.mint);
        let target = match self.answers {
            Some(answers) => target.answering(answers.clone()),
            None => target,
        };
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
            commands: target.commands(),
            answered: target.answered(),
            unarranged: target.unarranged(),
        })
    }
}

/// The suite of `scenario` alone, under `id`, in the format `ir` needs.
fn suite(ir: &EssIr, id: &ScenarioId, scenario: ConformanceScenario) -> Result<ConformanceSuite> {
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(ir));
    if let Err(id) = suite.insert(id.clone(), scenario) {
        bail!("`{id}` is already in the suite");
    }
    suite.select_fresh_format_for(ir);
    Ok(suite)
}

/// The first `end` compiled steps of `scenario`, its one-time trace restricted to them.
///
/// ESS attaches a trace to the whole scenario: every window is anchored to one of its operations
/// and every origin is invoked by one of its sends, so a prefix that keeps the whole trace is
/// refused (`InvalidOneTimeResponse`). The prefix keeps the windows anchored to its own steps, the
/// origins whose command it invokes and the required origins whose outcome it selects; with no
/// required origin left it carries no trace, as ESS attaches none. Each kept window and the event
/// catalog keep the whole scenario's `within_ms`, not the longest wait of the prefix's own steps
/// that ESS would take attaching a trace to the prefix alone: every replay of the bisection then
/// observes events under one bound, the one the whole scenario is replayed under.
fn prefix(scenario: &ConformanceScenario, end: usize) -> ConformanceScenario {
    let steps = scenario.steps[..end].to_vec();
    let one_time_response = scenario.one_time_response.as_ref().and_then(|trace| {
        let invoked = steps
            .iter()
            .filter_map(|step| match step {
                ScenarioStep::ExecuteCommand { command, .. }
                | ScenarioStep::ExecuteCommandWithoutInput { command, .. } => Some(command),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let selected = steps
            .iter()
            .filter_map(|step| match step {
                ScenarioStep::ExpectOutcome { outcome } => Some(outcome),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let origins = trace
            .origins
            .iter()
            .filter(|origin| invoked.contains(&origin.command))
            .cloned()
            .collect::<Vec<_>>();
        let required_origins = trace
            .required_origins
            .iter()
            .filter(|required| {
                selected.contains(required)
                    && origins.iter().any(|origin| &origin.outcome == *required)
            })
            .cloned()
            .collect::<Vec<_>>();
        (!required_origins.is_empty()).then(|| Trace {
            origins,
            required_origins,
            events: trace.events.clone(),
            event_windows: trace
                .event_windows
                .iter()
                .filter(|window| window.after_step < end)
                .cloned()
                .collect(),
        })
    });
    ConformanceScenario {
        steps,
        one_time_response,
        ..scenario.clone()
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

/// For each of `steps`, the command it sends, counted from 0 in the order the runner asks the
/// target to execute them: the first `execute_command` step of its compiled steps. A step that
/// executes no command (what is arranged, an assertion) sends none.
fn sends(compiled: &[ScenarioStep], steps: &[Step]) -> Vec<Option<usize>> {
    let (mut start, mut sent) = (0, 0);
    steps
        .iter()
        .map(|step| {
            let own = compiled[start..step.end]
                .iter()
                .filter(|step| matches!(step, ScenarioStep::ExecuteCommand { .. }))
                .count();
            start = step.end;
            let send = (own > 0).then_some(sent);
            sent += own;
            send
        })
        .collect()
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

    /// Every prefix of a compiled scenario whose external cause returns a one-time response is
    /// admitted by ESS (`story:one-time-response-admission`): it keeps only the windows anchored
    /// to its own steps and the origins it invokes.
    #[test]
    fn a_prefix_keeps_only_its_own_one_time_windows() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let ir = spindle_spec::load_with_implied_component(&fixtures.join("one-time-replay"))
            .expect("the specification compiles")
            .ir;
        let mut admitted = 0;
        for name in ["holds.yaml", "diverges.yaml"] {
            let path = fixtures.join("one-time-replay-scenarios").join(name);
            let document = std::fs::read_to_string(&path).expect("the scenario reads");
            let authoring = authored::compile(&ir, &[Source::new(name.to_owned(), document)]);
            assert!(authoring.is_complete(), "{:?}", authoring.refusals);
            let (id, compiled) = authoring
                .scenarios
                .into_iter()
                .next()
                .expect("one scenario");
            assert!(
                compiled.one_time_response.is_some(),
                "{name} carries a one-time trace"
            );
            for step in steps(&compiled.steps) {
                let suite = suite(&ir, &id, prefix(&compiled, step.end)).expect("a suite");
                if let Err(error) = AdmittedSuite::from_suite(&suite) {
                    panic!("{name} up to {}: {error:?}", step.label);
                }
                admitted += 1;
            }
        }
        assert_eq!(admitted, 5, "every document step's prefix is replayed");
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
