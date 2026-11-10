use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
        .expect("the commands are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

#[test]
fn run_admits_then_answers_each_command_line() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let state = directory.path().join("state.sqlite");
    let lines = concat!(
        r#"{"command": "spindle.deployment.DeclareEnvironment", "input": {"environment_id": "laptop", "os": "linux", "arch": "x86_64", "provides": [{"capability": "store.durable", "implementations": ["sqlite"]}]}}"#,
        "\n",
        r#"{"command": "spindle.deployment.OfferBundle", "input": {"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop"}}"#,
        "\n",
        r#"{"command": "spindle.deployment.DecideAdmission", "input": {"deployment_id": "d1", "requires": ["store.durable"], "bindings": [{"capability": "store.durable", "implementation": "sqlite"}], "missing": []}, "external": {"refused": false}}"#,
        "\n",
        r#"{"command": "spindle.deployment.DecideAdmission", "input": {"deployment_id": "d1", "requires": ["store.durable"], "bindings": [{"capability": "store.durable", "implementation": "sqlite"}], "missing": []}, "external": {"refused": false}}"#,
        "\n",
        r#"{"command": "spindle.deployment.Nothing", "input": {}}"#,
        "\n",
    );
    let output = run(
        &[
            "run",
            "ess",
            "--environment",
            "examples/environments/laptop.toml",
            "--state",
            state.to_str().expect("a UTF-8 path"),
        ],
        lines,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 5, "{results:?}");
    assert_eq!(results[0]["outcome"], "declared");
    assert_eq!(results[1]["outcome"], "offered");
    assert_eq!(
        results[1]["events"][0]["event"],
        "spindle.deployment.BundleOffered"
    );
    assert_eq!(results[2]["outcome"], "admitted");
    assert_eq!(results[3]["outcome"], "wrong-state");
    assert_eq!(
        results[3]["error"]["error"],
        "spindle.deployment.DeploymentStateConflict"
    );
    assert_eq!(results[4]["failure"]["kind"], "unknown_command");
}

#[test]
fn run_refuses_to_start_when_admission_is_refused() {
    let output = run(
        &[
            "run",
            "ess",
            "--environment",
            "examples/environments/microvm.toml",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "refused");
}

#[test]
fn run_refuses_a_state_file_when_the_store_is_memory() {
    let output = run(
        &[
            "run",
            "ess",
            "--environment",
            "crates/spindle-cli/tests/fixtures/memory.toml",
            "--state",
            "unused.sqlite",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("store.durable is bound to memory"),
        "{stderr}"
    );
    assert!(
        !root().join("unused.sqlite").exists(),
        "no state file is created"
    );
}

#[test]
fn run_keeps_memory_state_for_the_length_of_one_run() {
    let lines = concat!(
        r#"{"command": "spindle.deployment.DeclareEnvironment", "input": {"environment_id": "scratchpad", "os": "linux", "arch": "x86_64", "provides": []}}"#,
        "\n",
        r#"{"command": "spindle.deployment.OfferBundle", "input": {"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "scratchpad"}}"#,
        "\n",
        r#"{"command": "spindle.deployment.StartDeployment", "input": {"deployment_id": "d1"}}"#,
        "\n",
    );
    let output = run(
        &[
            "run",
            "ess",
            "--environment",
            "crates/spindle-cli/tests/fixtures/memory.toml",
        ],
        lines,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[0]["outcome"], "declared");
    assert_eq!(results[1]["outcome"], "offered");
    assert_eq!(
        results[2]["outcome"], "wrong-state",
        "an offered deployment is not admitted"
    );
}
