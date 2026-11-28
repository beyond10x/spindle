//! Adversary cases of wave 32 unit U2 (`story:conform-awaits-system-steps`): `run --scenario` over
//! an authored await, on the neutral `adversary-w32-u2` fixture of `spindle-conformance`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SPEC: &str = "crates/spindle-conformance/tests/fixtures/adversary-w32-u2";
const SCENARIOS: &str = "crates/spindle-conformance/tests/fixtures/adversary-w32-u2-scenarios";
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

/// An authored act expecting the refusal the system decides on an unbounded system step compiles
/// to `expect_not_issued`, which the runner asks until its deadline. ESS's interpreter answers
/// every ask the same, so the document holds there; the replay must hold too.
#[test]
fn a_replayed_unbounded_refusal_holds_as_on_the_interpreter() {
    let output = replay("dim.yaml");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        stdout(&output).contains("timeline[1] lamps.hall.Switch: holds\n"),
        "{}",
        stdout(&output)
    );
}

/// The await's act names no outcome beside it (D4 A), and still the replay checks what the act
/// claims of it: an event the system did not publish fails the replay at that act.
#[test]
fn a_replayed_await_claiming_the_wrong_crew_fails_at_its_act() {
    let output = replay("wrong-crew.yaml");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("timeline[2] lamps.hall.Install: holds"),
        "{printed}"
    );
    assert!(
        printed.contains("timeline[3] lamps.hall.Dispatch")
            && !printed.contains("timeline[3] lamps.hall.Dispatch: holds"),
        "{printed}"
    );
}

/// The replay of a served crew holds, and its await line names no outcome (D4 A).
#[test]
fn a_replayed_await_sends_the_free_crew() {
    let output = replay("served.yaml");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("timeline[3] lamps.hall.Dispatch: holds\n"),
        "{printed}"
    );
    assert!(
        printed.contains("assert[0] lamps.hall.Lamps: holds"),
        "{printed}"
    );
}
