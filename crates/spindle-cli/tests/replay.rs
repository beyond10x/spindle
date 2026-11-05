//! `run --scenario`: replaying an `ess-scenario/1` timeline, read by ESS's own reader.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/replay";
const SCENARIOS: &str = "crates/spindle-cli/tests/fixtures/replay-scenarios";
const LAPTOP: &str = "examples/environments/laptop.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn replay(scenario: &str) -> Output {
    replay_of(SPEC, scenario)
}

fn replay_of(spec: &str, scenario: &str) -> Output {
    let scenario = format!("{SCENARIOS}/{scenario}");
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            spec,
            "--environment",
            LAPTOP,
            "--scenario",
            &scenario,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_scenario_whose_steps_all_hold_exits_zero() {
    let output = replay("holds.yaml");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let printed = stdout(&output);
    for step in [
        "timeline[0] desk.ticket.OpenTicket: holds",
        "timeline[1] desk.ticket.OpenTicket: holds",
        "assert[0] desk.ticket.OpenTickets: holds",
        "assert[1] desk.ticket.OpenTickets: holds",
    ] {
        assert!(printed.contains(step), "{step:?} in {printed}");
    }
    assert!(!printed.contains("diverges"), "{printed}");
}

#[test]
fn a_capture_feeds_a_later_step() {
    let output = replay("capture.yaml");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("timeline[1] desk.ticket.CloseTicket: holds"),
        "the close sent with the captured id takes `closed`, not `no-such-ticket`: {printed}"
    );
    assert!(
        printed.contains("assert[0] desk.ticket.AllTickets: holds"),
        "{printed}"
    );
}

#[test]
fn the_first_divergent_step_is_named() {
    let output = replay("wrong-outcome.yaml");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("timeline[0] desk.ticket.OpenTicket: holds"),
        "{printed}"
    );
    assert!(
        printed.contains("timeline[1] desk.ticket.CloseTicket: diverges"),
        "{printed}"
    );
    assert!(
        printed.contains("outcome = already-closed"),
        "the expected outcome: {printed}"
    );
    assert!(
        printed.contains("outcome = closed"),
        "the outcome taken: {printed}"
    );
    assert!(
        !printed.contains("timeline[2]"),
        "no step after the divergent one runs: {printed}"
    );
    assert!(
        printed.contains("this replay ran 2 command(s), none after timeline[1]"),
        "the reported replay sent the open and the first close, and never the second close: {printed}"
    );
}

#[test]
fn a_view_assert_that_does_not_hold_is_a_divergence() {
    let output = replay("missing-row.yaml");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("assert[0] desk.ticket.OpenTickets: holds"),
        "{printed}"
    );
    assert!(
        printed.contains("assert[1] desk.ticket.OpenTickets: diverges"),
        "{printed}"
    );
    assert!(printed.contains("kettle"), "the row asserted: {printed}");
}

#[test]
fn a_scenario_ess_refuses_is_refused() {
    let output = replay("unknown-format.yaml");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert!(
        !printed.contains("timeline["),
        "no step runs before the document is read: {printed}"
    );
    let diagnostic = stderr(&output);
    assert!(
        diagnostic.contains("ESS-AUTHOR-002"),
        "ESS's own code: {diagnostic}"
    );
    assert!(
        diagnostic.contains("ess-scenario/999"),
        "the format named: {diagnostic}"
    );
}

#[test]
fn a_scenario_of_more_than_a_thousand_steps_is_refused_before_any_replay() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut text = String::from(
        "type: ess-scenario/1\ndomain: desk.ticket\nscenario: too-many-openings\n\
         summary: More tickets open than a replay takes.\ntimeline:\n",
    );
    for act in 0..1001_usize {
        text.push_str(&format!(
            "  - at: 2026-01-05T{:02}:{:02}:{:02}Z\n    command: desk.ticket.OpenTicket\n    \
             input:\n      title: t{act}\n    outcome: opened\n",
            act / 3600,
            (act / 60) % 60,
            act % 60
        ));
    }
    let scenario = directory.path().join("too-long.yaml");
    std::fs::write(&scenario, text).expect("written");
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            SPEC,
            "--environment",
            LAPTOP,
            "--scenario",
            scenario.to_str().expect("a UTF-8 path"),
        ])
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stdout(&output).is_empty(), "no step ran: {output:?}");
    let diagnostic = stderr(&output);
    assert!(
        diagnostic.contains("has 1001 steps; a replay takes at most 1000"),
        "{diagnostic}"
    );
}

/// A specification whose binding maps `event.member_id` into a `reader_id` of another type across a
/// declared conversion replays from its first step: the binding is routed, not refused at startup,
/// and its command runs.
#[test]
fn replay_starts_on_a_specification_with_converted_mappings() {
    let output = replay_of(
        "crates/spindle-channel/tests/fixtures/library",
        "converted.yaml",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let printed = stdout(&output);
    for step in [
        "timeline[0] library.lending.CloseLoan: holds",
        "assert[0] library.lending.Loans: holds",
        "assert[1] library.notices.AllThanks: holds",
    ] {
        assert!(printed.contains(step), "{step:?} in {printed}");
    }
    assert!(!printed.contains("diverges"), "{printed}");
}
