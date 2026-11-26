//! `run` over a specification whose creations mint their identity.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FIXTURE: &str = "crates/spindle-kernel/tests/fixtures/mint";

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
fn run_mints_each_new_identity_a_uuid_newtype_and_a_bare_string() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let state = directory.path().join("state.sqlite");
    let lines = concat!(
        r#"{"command": "mint.ticket.OpenTicket", "input": {"title": "first"}}"#,
        "\n",
        r#"{"command": "mint.ticket.OpenTicket", "input": {"title": "second"}}"#,
        "\n",
        r#"{"command": "mint.label.OpenLabel", "input": {"text": "fragile"}}"#,
        "\n",
    );
    let output = run(
        &[
            "run",
            FIXTURE,
            "--environment",
            "examples/environments/laptop.toml",
            "--state",
            state.to_str().expect("a UTF-8 path"),
        ],
        lines,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    let minted = results[..2]
        .iter()
        .map(|result| {
            assert_eq!(result["outcome"], "opened", "{result}");
            let identity = result["events"][0]["payload"]["ticket_id"]
                .as_str()
                .unwrap_or_else(|| panic!("a minted identity in {result}"));
            assert_ne!(
                result["events"][1]["payload"]["ticket_id"], identity,
                "only the event `instance:` names carries the identity"
            );
            identity.to_owned()
        })
        .collect::<Vec<_>>();
    assert_ne!(minted[0], minted[1], "each creation mints its own identity");

    let label = &results[2];
    assert_eq!(label["outcome"], "opened", "{label}");
    let identity = label["events"][0]["payload"]["label_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a minted text identity in {label}"));
    assert!(
        identity.len() == 36 && identity.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "a bare `String` identity is a UUID's text with no prefix: {identity}"
    );
}

#[test]
fn run_refuses_a_specification_that_mints_on_an_environment_without_identity_mint() {
    let output = run(
        &[
            "run",
            FIXTURE,
            "--environment",
            "crates/spindle-cli/tests/fixtures/memory.toml",
        ],
        "",
    );
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "refused");
    assert_eq!(results[0]["missing"], serde_json::json!(["identity.mint"]));
}
