use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .output()
        .expect("b10x-spindle runs")
}

#[test]
fn metrics_prints_the_catalogue_of_a_component() {
    let output = spindle(&[
        "metrics",
        "examples/rally/ess",
        "--component",
        "rally-table",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in [
        "command.rally.table.OpenMatch.opened counter kernel",
        "command.rally.table.Tick.duration_us histogram kernel",
        "event.rally.table.BallMoved.appended counter store",
        "binding.west-watches.sent counter channel",
        "timer.tick.fired counter timer",
        "store.bytes_written counter store",
    ] {
        assert!(
            stdout.lines().any(|printed| printed == line),
            "{line}\n{stdout}"
        );
    }

    let output = spindle(&[
        "metrics",
        "examples/rally/ess",
        "--component",
        "rally-player-west",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let catalogue: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(catalogue["component"], "rally-player-west");
    assert!(
        catalogue["counters"]
            .as_array()
            .expect("a list")
            .iter()
            .any(
                |counter| counter["name"] == "binding.west-watches.delivered"
                    && counter["source"] == "channel"
            )
    );
}

#[test]
fn metrics_of_an_unknown_component_is_an_error() {
    let output = spindle(&["metrics", "examples/rally/ess", "--component", "nowhere"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("no component"));
}
