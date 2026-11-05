//! The caller a command is sent as, supplied per invocation beside its input: `"caller": {...}`
//! on a `run` stdin line, and a scenario step's `caller` under `run --scenario`.
//!
//! The specification is the kernel's fixture `crates/spindle-kernel/tests/fixtures/caller`: the
//! actor `Writer` carries `owner_id`, `CreateNotebook` records it, and `EditNotebook` refuses with
//! `not-yours` a writer who is not the owner.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SPEC: &str = "crates/spindle-kernel/tests/fixtures/caller";
const SCENARIOS: &str = "crates/spindle-cli/tests/fixtures/caller-scenarios";
const LAPTOP: &str = "examples/environments/laptop.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str], lines: &str) -> Output {
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

#[test]
fn replay_passes_a_step_s_caller() {
    let scenario = format!("{SCENARIOS}/owner-edits.yaml");
    let output = spindle(
        &[
            "run",
            SPEC,
            "--environment",
            LAPTOP,
            "--scenario",
            &scenario,
        ],
        "",
    );
    let printed = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for step in [
        "timeline[0] quill.notebook.CreateNotebook: holds",
        "timeline[1] quill.notebook.EditNotebook: holds",
        "timeline[2] quill.notebook.EditNotebook: holds",
        "assert[0] quill.notebook.NotebookCard: holds",
    ] {
        assert!(printed.contains(step), "{step:?} in {printed}");
    }
    assert!(!printed.contains("unsupported"), "{printed}");
}

#[test]
fn run_reads_a_line_s_caller_beside_its_input() {
    let lines = concat!(
        r#"{"command": "quill.notebook.CreateNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "field notes"}, "caller": {"owner_id": "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "stolen"}, "caller": {"owner_id": "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "fair copy"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "fair copy"}, "caller": {"owner_id": "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0"}}"#,
        "\n",
    );
    let output = spindle(&["run", SPEC, "--environment", LAPTOP], lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect();
    assert_eq!(results.len(), 4, "{results:?}");
    assert_eq!(results[0]["outcome"], "created", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["payload"]["owner_id"], "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0",
        "{results:?}"
    );
    assert_eq!(results[1]["outcome"], "not-yours", "{results:?}");
    assert_eq!(results[1]["error"]["error"], "quill.notebook.NotYours");
    assert!(
        results[2]["failure"].is_object(),
        "a line with no caller selects no branch the caller decides: {results:?}"
    );
    assert_eq!(results[3]["outcome"], "edited", "{results:?}");
}

#[test]
fn a_route_whose_command_reads_the_caller_stays_unserved() {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["shelf".to_owned()], None)
        .expect("the fixture's surface is routed");
    let unserved: Vec<String> = surface
        .unserved()
        .iter()
        .map(|route| {
            format!(
                "{}: {}",
                route.operation,
                route
                    .unserved()
                    .map_or_else(String::new, ToString::to_string)
            )
        })
        .collect();
    assert_eq!(
        unserved.len(),
        3,
        "every command of the fixture reads its caller: {unserved:#?}"
    );
    for route in &unserved {
        assert!(
            route.contains(
                "this ingress does not serve the caller attributes it reads (`owner_id`)"
            ),
            "how an HTTP caller proves its attributes is not decided, so the route answers 501: \
             {route}"
        );
    }
}
