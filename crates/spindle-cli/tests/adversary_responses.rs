//! Adversary cases for `story:kernel-command-responses`: the response the kernel answers must
//! reach every caller ESS checks, not only the HTTP ingress.
//!
//! The fixture is the kernel's `tests/fixtures/responses`, whose `ShareNotebook/shared` returns
//! `{link, share_code}` with `one_time_response: [share_code]`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

const SPEC: &str = "crates/spindle-kernel/tests/fixtures/responses";
const LAPTOP: &str = "examples/environments/laptop.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `conform` runs the suite ESS synthesises for the fixture against Spindle's in-process target.
/// ESS's runner checks the actual returned response of a returning outcome against its declared
/// fields (`SemanticCommandResult::response`); a target that runs the outcome and answers no
/// response fails every such check. No executed scenario may fail on a response the kernel did
/// answer.
#[test]
fn conform_hands_the_kernels_response_to_the_suite() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let results = dir.path().join("results.json");
    let report = dir.path().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["conform", SPEC, "--environment", LAPTOP, "--results"])
        .arg(&results)
        .arg("--report")
        .arg(&report)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs");
    let text = std::fs::read_to_string(&results).unwrap_or_else(|error| {
        panic!(
            "conform wrote no results ({error}): {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let results: Value = serde_json::from_str(&text).expect("the results are JSON");
    let scenarios = results["scenarios"].as_array().expect("scenarios");
    let executed: Vec<&Value> = scenarios
        .iter()
        .filter(|scenario| scenario["status"] != "unsupported")
        .collect();
    assert!(
        !executed.is_empty(),
        "the suite executes a scenario: {results}"
    );
    let failed: Vec<String> = executed
        .iter()
        .filter(|scenario| scenario["status"] != "passed")
        .map(|scenario| {
            let reasons: Vec<String> = scenario["checks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|check| check["status"] != "passed")
                .map(Value::to_string)
                .collect();
            format!(
                "{} {}: {}",
                scenario["scenario"],
                scenario["status"],
                reasons.join("; ")
            )
        })
        .collect();
    assert!(
        failed.is_empty(),
        "conform exited {:?}; executed scenarios that did not pass:\n{}",
        output.status.code(),
        failed.join("\n")
    );
}
