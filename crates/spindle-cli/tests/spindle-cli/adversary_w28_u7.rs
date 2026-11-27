//! Adversary cases for wave 28, unit U7 (`story:one-time-response-admission`,
//! `story:replay-with-decisions`).
//!
//! The one-time cases bisect scenarios whose early prefixes keep no required origin: one invokes
//! the one-time command without selecting its one-time outcome, one invokes no one-time command at
//! all. ESS attaches no trace to such a prefix, and a prefix that kept one would either be refused
//! (`InvalidOneTimeResponse`, no observed origin required) or fail its own required origin, naming
//! the wrong step. The decisions cases pin that a rule on the input answers in a replay, that the
//! file answers an act whose stated outcome forces nothing, and that a replay without a file, or
//! with an empty one, answers as ESS's interpreter does.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, AdvancingClock, ConformanceSuite, Ids, Runner, RunnerConfig, Status,
    SuiteProvenance,
};

const ONE_TIME: &str = "crates/spindle-cli/tests/fixtures/one-time-replay";
const REVIEW: &str = "crates/spindle-cli/tests/fixtures/review";
const REVIEW_SCENARIOS: &str = "crates/spindle-cli/tests/fixtures/review-scenarios";
const OWN: &str = "crates/spindle-cli/tests/fixtures/adversary-w28-u7";
const LAPTOP: &str = "examples/environments/laptop.toml";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `run <spec> --environment <environment> --scenario <scenario>`, with `decisions` written as
/// the decisions file where given.
fn replay(spec: &str, environment: &str, scenario: &str, decisions: Option<&str>) -> Output {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command.current_dir(root()).args([
        "run",
        spec,
        "--environment",
        environment,
        "--scenario",
        scenario,
    ]);
    if let Some(text) = decisions {
        let file = directory.path().join("decisions.toml");
        std::fs::write(&file, text).expect("the decisions file is written");
        command.arg("--decisions").arg(&file);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

/// The status ESS 0.99.1's own runner gives the scenario at `scenario` against ESS's interpreter
/// of `spec`.
fn ess_status(spec: &str, scenario: &str) -> Status {
    let ir = spindle_spec::load(&root().join(spec))
        .expect("the specification compiles")
        .ir;
    let document = std::fs::read_to_string(root().join(scenario)).expect("the scenario reads");
    let authoring = authored::compile(&ir, &[Source::new(scenario.to_owned(), document)]);
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(&ir));
    for (id, compiled) in authoring.scenarios {
        assert!(
            compiled.one_time_response.is_some() || !spec.ends_with("one-time-replay"),
            "{scenario} carries a one-time trace"
        );
        suite.insert(id, compiled).expect("one scenario");
    }
    suite.select_fresh_format_for(&ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let target = Interpreted::for_model(ir.clone());
    Runner::new(
        RunnerConfig::default(),
        AdvancingClock::default(),
        Ids::for_suite(&suite),
    )
    .run_admitted(&admitted, &target)
    .into_report()
    .scenarios[0]
        .status
}

fn assert_lines(output: &Output, lines: &[&str]) {
    let printed = stdout(output);
    for line in lines {
        assert!(printed.contains(line), "{line:?} in {printed}");
    }
}

/// The first prefix invokes `IssueKey` but selects none of its outcomes, so no origin is required
/// there: ESS attaches no trace to it. Keeping the trace there is refused, or requires `issued`
/// and fails at `timeline[0]`.
#[test]
fn a_prefix_that_selects_no_one_time_outcome_carries_no_trace() {
    let scenario = format!("{OWN}/unselected-origin-first.yaml");
    assert_eq!(ess_status(ONE_TIME, &scenario), Status::Failed);
    let output = replay(ONE_TIME, LAPTOP, &scenario, None);
    assert!(
        !stderr(&output).contains("InvalidOneTimeResponse"),
        "no prefix is refused: {}",
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_lines(
        &output,
        &[
            "timeline[0] vault.key.IssueKey: holds, outcome held, error vault.key.Held\n",
            "timeline[1] vault.key.IssueKey: holds, outcome issued\n",
            "timeline[2] vault.key.RevokeKey: diverges, outcome revoked\n",
        ],
    );
}

/// The first prefix invokes no one-time command: it keeps no origin, so ESS attaches no trace.
#[test]
fn a_prefix_that_invokes_no_one_time_command_carries_no_trace() {
    let scenario = format!("{OWN}/no-origin-first.yaml");
    assert_eq!(ess_status(ONE_TIME, &scenario), Status::Failed);
    let output = replay(ONE_TIME, LAPTOP, &scenario, None);
    assert!(
        !stderr(&output).contains("InvalidOneTimeResponse"),
        "no prefix is refused: {}",
        stderr(&output)
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_lines(
        &output,
        &[
            "timeline[0] vault.key.RevokeKey: holds, outcome no-such-key, error vault.key.NoSuchKey\n",
            "timeline[1] vault.key.IssueKey: holds, outcome issued\n",
            "timeline[2] vault.key.RevokeKey: diverges, outcome revoked\n",
        ],
    );
}

/// A `[[rules]]` entry answers by the act's input in a replay, as it does in a stdin `run`.
#[test]
fn a_rule_on_the_input_answers_each_act_by_its_own_input() {
    let rules = concat!(
        "[[rules]]\n",
        "command = \"probe.review.Submit\"\n",
        "outcome = \"flagged\"\n",
        "answer = true\n",
        "when = { \"input.submission_id\" = \"s1\" }\n",
    );
    let output = replay(
        REVIEW,
        MEMORY,
        &format!("{OWN}/two-submissions.yaml"),
        Some(rules),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_lines(
        &output,
        &[
            "timeline[0] probe.review.Submit: holds, outcome flagged, error probe.review.Flagged\n",
            "timeline[1] probe.review.Submit: holds, outcome submitted\n",
        ],
    );
}

/// An act stating a branch no external cause decides forces nothing (ESS arms no answer for it),
/// so the file answers its causes, as `run --help` says: the act diverges.
#[test]
fn the_file_answers_an_act_whose_stated_outcome_forces_nothing() {
    let scenario = format!("{OWN}/stated-default.yaml");
    assert_eq!(ess_status(REVIEW, &scenario), Status::Passed);
    let output = replay(
        REVIEW,
        MEMORY,
        &scenario,
        Some("[\"probe.review.Submit\"]\nflagged = true\n"),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_lines(
        &output,
        &[
            "timeline[0] probe.review.Submit: diverges, outcome flagged, error probe.review.Flagged\n",
        ],
    );
}

/// Without `--decisions`, and with an empty decisions file, a cause nothing forces does not hold,
/// as ESS's interpreter answers it: the replay prints the same lines either way.
#[test]
fn no_file_and_an_empty_file_answer_as_ess_does() {
    let scenario = format!("{REVIEW_SCENARIOS}/unforced.yaml");
    assert_eq!(ess_status(REVIEW, &scenario), Status::Passed);
    let without = replay(REVIEW, MEMORY, &scenario, None);
    assert_eq!(without.status.code(), Some(0), "{without:?}");
    assert_lines(
        &without,
        &["timeline[0] probe.review.Submit: holds, outcome submitted\n"],
    );
    let empty = replay(REVIEW, MEMORY, &scenario, Some(""));
    assert_eq!(empty.status.code(), Some(0), "{empty:?}");
    assert_eq!(stdout(&empty), stdout(&without));
}

/// `--answer-default` stays refused beside `--scenario`, with or without `--decisions`.
#[test]
fn answer_default_stays_refused_with_a_decisions_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            REVIEW,
            "--environment",
            MEMORY,
            "--scenario",
            &format!("{REVIEW_SCENARIOS}/unforced.yaml"),
            "--decisions",
            "x.toml",
            "--answer-default",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("--answer-default"), "{output:?}");
}
