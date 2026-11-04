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
    let scenario = format!("{SCENARIOS}/{scenario}");
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            SPEC,
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
