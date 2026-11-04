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
fn admitted_exits_zero_and_prints_the_bindings() {
    let output = spindle(&[
        "admit",
        "ess",
        "--environment",
        "examples/environments/laptop.toml",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let admission: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(admission["outcome"], "admitted");
    assert_eq!(admission["bindings"].as_array().map(Vec::len), Some(3));
}

#[test]
fn refused_exits_three_and_names_the_missing_capability() {
    let output = spindle(&[
        "admit",
        "ess",
        "--environment",
        "examples/environments/microvm.toml",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let admission: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(admission["outcome"], "refused");
    assert_eq!(
        admission["missing"],
        serde_json::json!([
            "external.spindle.deployment.DecideAdmission.refused",
            "external.spindle.placement.PlaceInstance.refused"
        ])
    );
}

#[test]
fn an_unknown_component_is_an_error() {
    let output = spindle(&["capabilities", "ess", "--component", "nowhere"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("no component"));
}
