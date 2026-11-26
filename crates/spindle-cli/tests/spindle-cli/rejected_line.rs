//! `run` reading JSON lines on stdin exits 1 when a line was rejected, after it answered every
//! line, and names each rejected line on stderr (`story:run-fails-on-rejected-line`). A line is
//! rejected when its result carries `failure`: it is not JSON, names no declared command, its input
//! is not admitted, its external cause is unanswered, or the kernel refused it before an outcome.
//! A line answered by a declared outcome, a refusing one included, is not rejected.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";
const DECLARE: &str = r#"{"command": "spindle.deployment.DeclareEnvironment", "input": {"environment_id": "scratchpad", "os": "linux", "arch": "x86_64", "provides": []}}"#;
const OFFER: &str = r#"{"command": "spindle.deployment.OfferBundle", "input": {"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "scratchpad"}}"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(arguments: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the lines are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

/// Lines 1 and 3 run and line 2 names an undeclared command: all three print their result, and
/// the run exits 1 after the last line, naming line 2 on stderr and no other.
#[test]
fn a_stdin_run_with_a_rejected_line_exits_1() {
    let lines = format!(
        "{DECLARE}\n{}\n{OFFER}\n",
        r#"{"command": "spindle.deployment.Nothing", "input": {}}"#
    );
    let output = run(&["run", "ess", "--environment", MEMORY], &lines);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "every line is answered: {results:?}");
    assert_eq!(results[0]["outcome"], "declared");
    assert_eq!(results[1]["failure"]["kind"], "unknown_command");
    assert_eq!(
        results[2]["outcome"], "offered",
        "the line after the rejected one runs"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let named: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("rejected"))
        .collect();
    assert!(
        named.iter().any(|line| line.contains("line 2")),
        "stderr names line 2: {stderr}"
    );
    assert!(
        named
            .iter()
            .all(|line| !line.contains("line 1") && !line.contains("line 3")),
        "only line 2 is named: {stderr}"
    );
}

/// A line that is not JSON is rejected too, and a blank line is not counted: the run exits 1 and
/// names the line by its place in the input.
#[test]
fn a_line_that_is_not_json_is_rejected() {
    let lines = format!("{DECLARE}\n\n{{\"command\": \n");
    let output = run(&["run", "ess", "--environment", MEMORY], &lines);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[1]["failure"]["kind"], "invalid_request");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("rejected") && line.contains("line 3")),
        "stderr names line 3, the blank line 2 included in the count: {stderr}"
    );
}

/// A line answered by a declared refusing outcome is not a rejection: the exit status stays 0.
#[test]
fn a_declared_refusal_is_not_a_rejection() {
    let lines = format!(
        "{DECLARE}\n{OFFER}\n{}\n",
        r#"{"command": "spindle.deployment.StartDeployment", "input": {"deployment_id": "d1"}}"#
    );
    let output = run(&["run", "ess", "--environment", MEMORY], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[2]["outcome"], "wrong-state", "{results:?}");
    assert!(
        results[2]["error"].is_object(),
        "the refusing outcome declares its error: {results:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("rejected"), "{stderr}");
}
