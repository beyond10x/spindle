//! Adversary cases of wave 32 unit U2 (`story:conform-awaits-system-steps`): an awaited system
//! step on `HostTarget`, held to ESS's interpreter (`Interpreted::for_model`, ESS 0.99.2) over the
//! neutral `adversary-w32-u2` fixture.
//!
//! The fixture's `Switch` is a system step with no bound whose input `bright`, a `Boolean` no
//! scenario states, the system decides by minting; `Dispatch` is bounded, picks a crew through a
//! `when_related:` input, and has an `external:` refusal a scenario may force.

use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, ConformanceSuite, ConformanceTarget, Status, SuiteProvenance,
};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FIXTURE: &str = "tests/fixtures/adversary-w32-u2";
const SCENARIOS: &str = "tests/fixtures/adversary-w32-u2-scenarios";

fn specification() -> spindle_spec::Specification {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// The status ESS's runner gives the authored document `file` on `target`, with its report.
fn replay<T: ConformanceTarget>(ir: &EssIr, file: &str, target: &T) -> (Status, String) {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(SCENARIOS)
            .join(file),
    )
    .expect("the document reads");
    let authoring = authored::compile(ir, &[Source::new(file, text)]);
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(ir));
    for (id, scenario) in authoring.scenarios {
        suite.insert(id, scenario).expect("one scenario");
    }
    suite.select_fresh_format_for(ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let run = spindle_conformance::execute(&admitted, target);
    let result = run
        .report()
        .scenarios
        .first()
        .expect("one scenario")
        .clone();
    let mut rendered = Vec::new();
    run.render(&mut rendered).expect("the run renders");
    (result.status, String::from_utf8(rendered).expect("UTF-8"))
}

/// The authored `file` passes on ESS's interpreter and on the host alike.
fn passes_on_both(file: &str) {
    let ir = specification().ir;
    let (status, report) = replay(&ir, file, &Interpreted::for_model(ir.clone()));
    assert_eq!(status, Status::Passed, "ESS's interpreter: {report}");
    let (status, report) = replay(&ir, file, &HostTarget::new(ir.clone(), laptop()));
    assert_eq!(status, Status::Passed, "the host: {report}");
}

/// ESS's interpreter holds the store as it was before an await and mints from it each time it is
/// asked, so "asking again answers the same" (`interpret/await_outcome.rs`, *The hold*): the runner
/// asks an unbounded await until its deadline, and a system that decided the lamp is dim on the
/// first ask decides so on every ask. The host mints from a counter of its own that every ask
/// advances, so the minted `bright` alternates and a later ask lights the lamp.
#[test]
fn an_unbounded_await_asked_again_answers_the_same() {
    passes_on_both("dim.yaml");
}

/// A forced `external:` refusal is the step the system takes, past a candidate the model refuses
/// first (the busy crew): the forcing does not lapse at a refused candidate.
#[test]
fn a_forced_external_outcome_is_reached_past_a_refused_candidate() {
    passes_on_both("outage.yaml");
}

/// The held rows a `when_related:` input reads are tried in order, and the free crew is sent past
/// the busy one, its identity carried into the event and the stored row.
#[test]
fn the_free_crew_is_sent_past_a_busy_one() {
    passes_on_both("served.yaml");
}

/// A refusal the rows decide for every candidate is nothing issued within the bound
/// (deviation 3 of the implementor: `busy` has no synthesised scenario, `ESS-SYNTH-024`).
#[test]
fn a_busy_crew_is_not_sent_within_the_bound() {
    passes_on_both("busy.yaml");
}

/// Every scenario ESS synthesises from the fixture comes to the same status on the host as on
/// ESS's interpreter, and none of the host's is skipped.
#[test]
fn the_synthesised_suite_ends_as_on_the_interpreter() {
    let specification = specification();
    let suite = spindle_conformance::prepare(&specification, None)
        .expect("the fixture's suite is admitted")
        .suite;
    let host =
        spindle_conformance::execute(&suite, &HostTarget::new(specification.ir.clone(), laptop()));
    let interpreted =
        spindle_conformance::execute(&suite, &Interpreted::for_model(specification.ir.clone()));
    let statuses = |run: &spindle_conformance::Run| {
        run.report()
            .scenarios
            .iter()
            .map(|result| (result.scenario.to_string(), result.status))
            .collect::<Vec<_>>()
    };
    let mut rendered = Vec::new();
    host.render(&mut rendered).expect("the run renders");
    let rendered = String::from_utf8(rendered).expect("UTF-8");
    assert!(
        statuses(&host)
            .iter()
            .any(|(id, _)| id.starts_with("lamps.hall.Dispatch/")),
        "the suite awaits Dispatch: {rendered}"
    );
    assert_eq!(statuses(&host), statuses(&interpreted), "{rendered}");
    assert_eq!(host.counts().skipped, 0, "{rendered}");
}

// Round 2: the fix that keeps the mint counter only where an await reaches a step.

const ROUND_TWO: &str = "tests/fixtures/adversary-w32-u2-r2";
const ROUND_TWO_SCENARIOS: &str = "tests/fixtures/adversary-w32-u2-r2-scenarios";
const PERIODIC: &str = "tests/fixtures/adversary-w32-u2-periodic";

fn loaded(fixture: &str) -> spindle_spec::Specification {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture))
        .expect("the fixture loads")
}

/// The status ESS's runner gives the authored document `file` under `scenarios` on `target`.
fn replay_from<T: ConformanceTarget>(
    ir: &EssIr,
    scenarios: &str,
    file: &str,
    target: &T,
) -> (Status, String) {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(scenarios)
            .join(file),
    )
    .expect("the document reads");
    let authoring = authored::compile(ir, &[Source::new(file, text)]);
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(ir));
    for (id, scenario) in authoring.scenarios {
        suite.insert(id, scenario).expect("one scenario");
    }
    suite.select_fresh_format_for(ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let run = spindle_conformance::execute(&admitted, target);
    let status = run.report().scenarios.first().expect("one scenario").status;
    let mut rendered = Vec::new();
    run.render(&mut rendered).expect("the run renders");
    (status, String::from_utf8(rendered).expect("UTF-8"))
}

/// The round-two document `file` passes on ESS's interpreter and on the host alike.
fn round_two_passes_on_both(file: &str) {
    let ir = loaded(ROUND_TWO).ir;
    let (status, report) = replay_from(
        &ir,
        ROUND_TWO_SCENARIOS,
        file,
        &Interpreted::for_model(ir.clone()),
    );
    assert_eq!(status, Status::Passed, "ESS's interpreter: {report}");
    let (status, report) = replay_from(
        &ir,
        ROUND_TWO_SCENARIOS,
        file,
        &HostTarget::new(ir.clone(), laptop()),
    );
    assert_eq!(status, Status::Passed, "the host: {report}");
}

/// An await that reached nothing spends no value: the stamp after it carries the first code
/// (tick 1, `2`), as on ESS's interpreter.
#[test]
fn an_await_after_one_that_reached_nothing_mints_the_first_value() {
    round_two_passes_on_both("after-nothing.yaml");
}

/// Two awaits that each reach a step mint the first and the second value (`2`, then `3`).
#[test]
fn two_reached_awaits_mint_successive_values() {
    round_two_passes_on_both("two-awaits.yaml");
}

/// The documented deviation, from an authored document: a value the scenario's earlier command
/// generated (the `Tagged` serial) advances ESS's interpreter's counter, so its stamp carries the
/// second value (`3`); the host counts only the awaits' values.
#[test]
fn an_await_after_a_generated_value_mints_as_the_interpreter() {
    round_two_passes_on_both("after-generated.yaml");
}

/// The same fixture's synthesised suite ends as on ESS's interpreter: synthesis compares an
/// awaited input by what the target reported, so the deviation above is not reached from it.
#[test]
fn the_round_two_synthesised_suite_ends_as_on_the_interpreter() {
    let specification = loaded(ROUND_TWO);
    let suite = spindle_conformance::prepare(&specification, None)
        .expect("the fixture's suite is admitted")
        .suite;
    let host =
        spindle_conformance::execute(&suite, &HostTarget::new(specification.ir.clone(), laptop()));
    let interpreted =
        spindle_conformance::execute(&suite, &Interpreted::for_model(specification.ir.clone()));
    let statuses = |run: &spindle_conformance::Run| {
        run.report()
            .scenarios
            .iter()
            .map(|result| (result.scenario.to_string(), result.status))
            .collect::<Vec<_>>()
    };
    let mut rendered = Vec::new();
    host.render(&mut rendered).expect("the run renders");
    let rendered = String::from_utf8(rendered).expect("UTF-8");
    assert!(
        statuses(&host)
            .iter()
            .any(|(id, _)| id.starts_with("lamps.yard.Stamp/")),
        "the suite awaits Stamp: {rendered}"
    );
    assert_eq!(statuses(&host), statuses(&interpreted), "{rendered}");
}

/// A model with a periodic binding: nothing reached within a bound is unsupported on both, never
/// passed by the host (`target.rs`, the periodic check of `await_outcome`).
#[test]
fn a_bounded_await_beside_a_periodic_binding_is_not_passed() {
    let ir = loaded(PERIODIC).ir;
    let (expected, reference) = replay_from(
        &ir,
        SCENARIOS,
        "busy.yaml",
        &Interpreted::for_model(ir.clone()),
    );
    let (status, report) = replay_from(
        &ir,
        SCENARIOS,
        "busy.yaml",
        &HostTarget::new(ir.clone(), laptop()),
    );
    assert_ne!(status, Status::Passed, "the host: {report}");
    assert_eq!(
        status, expected,
        "the host: {report}\nESS's interpreter: {reference}"
    );
}
