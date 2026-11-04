//! `run --decisions <file>` and `--answer-default`: external outcomes answered without a per-line
//! `external` object. The fixture `tests/fixtures/review` declares `probe.review.Submit`, whose two
//! external outcomes sit beside a default branch, and `probe.review.Decide`, whose external outcome
//! has no default beside it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/review";
const ENVIRONMENT: &str = "crates/spindle-cli/tests/fixtures/memory.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(extra: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", ENVIRONMENT])
        .args(extra)
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

/// Writes `text` as a decisions file in a fresh directory and returns both.
fn decisions(text: &str) -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    let path = path.to_str().expect("a UTF-8 path").to_owned();
    (directory, path)
}

fn submit(id: &str, external: Option<&str>) -> String {
    let external =
        external.map_or_else(String::new, |answers| format!(r#", "external": {answers}"#));
    format!(
        r#"{{"command": "probe.review.Submit", "input": {{"submission_id": "{id}", "amount": 5}}{external}}}"#
    ) + "\n"
}

fn decide(id: &str, approve: bool) -> String {
    format!(
        r#"{{"command": "probe.review.Decide", "input": {{"decision_id": "{id}", "approve": {approve}}}}}"#
    ) + "\n"
}

#[test]
fn a_decisions_file_answers_every_external_outcome() {
    let (_directory, file) = decisions(concat!(
        "flagged = false\n",
        "\n",
        "[\"probe.review.Submit\"]\n",
        "escalated = true\n",
        "\n",
        "[probe.review.Decide]\n",
        "vetoed = false\n",
    ));
    let lines = submit("s1", None) + &decide("d1", true);
    let output = run(&["--decisions", &file], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[0]["outcome"], "escalated", "{results:?}");
    assert_eq!(
        results[0]["error"]["error"], "probe.review.Escalated",
        "{results:?}"
    );
    assert_eq!(results[1]["outcome"], "approved", "{results:?}");
    assert_eq!(
        results[1]["events"][0]["event"], "probe.review.Approved",
        "{results:?}"
    );
}

#[test]
fn answer_default_takes_the_default_branch() {
    let unanswered = run(&[], &submit("s1", None));
    assert_eq!(unanswered.status.code(), Some(0), "{unanswered:?}");
    let unanswered = results(&unanswered);
    assert_eq!(
        unanswered[0]["failure"]["kind"], "undecided",
        "without --answer-default nothing answers: {unanswered:?}"
    );

    let output = run(&["--answer-default"], &submit("s1", None));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "submitted", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["event"], "probe.review.Submitted",
        "{results:?}"
    );
}

#[test]
fn an_unanswered_external_without_a_default_fails_the_line() {
    let lines = decide("d1", true) + &decide("d2", false) + &submit("s1", None);
    let output = run(&["--answer-default"], &lines);
    assert_eq!(output.status.code(), Some(0), "the run goes on: {output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(results[0]["command"], "probe.review.Decide", "{results:?}");
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    let message = results[0]["failure"]["message"]
        .as_str()
        .expect("a failure message");
    assert!(message.contains("vetoed"), "names the outcome: {message}");
    assert!(
        message.contains("no default"),
        "says why --answer-default gave no answer: {message}"
    );
    assert_eq!(
        results[1]["outcome"], "declined",
        "a request the model turns away first asks nothing: {results:?}"
    );
    assert_eq!(results[2]["outcome"], "submitted", "{results:?}");
}

#[test]
fn a_request_answer_wins_over_the_file_and_the_file_over_the_default() {
    let (_directory, file) = decisions("[\"probe.review.Submit\"]\nescalated = true\n");
    let lines = submit("s1", None)
        + &submit("s2", Some(r#"{"escalated": false}"#))
        + &submit("s3", Some(r#"{"flagged": true}"#));
    let output = run(&["--decisions", &file, "--answer-default"], &lines);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(
        results[0]["outcome"], "escalated",
        "the file wins over the default: {results:?}"
    );
    assert_eq!(
        results[1]["outcome"], "submitted",
        "the request wins over the file: {results:?}"
    );
    assert_eq!(
        results[2]["outcome"], "flagged",
        "the request answers first: {results:?}"
    );
}

#[test]
fn a_command_table_wins_over_an_answer_for_every_command() {
    let (_directory, file) = decisions(
        "flagged = false\nescalated = true\n\n[\"probe.review.Submit\"]\nescalated = false\n",
    );
    let output = run(&["--decisions", &file], &submit("s1", None));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[0]["outcome"], "submitted", "{results:?}");
}

#[test]
fn a_decisions_file_naming_no_external_outcome_is_refused_before_running() {
    for (text, named) in [
        ("[\"probe.review.Submit\"]\nsubmitted = true\n", "submitted"),
        (
            "[\"probe.review.Missing\"]\nflagged = true\n",
            "probe.review.Missing",
        ),
        ("unknown = true\n", "unknown"),
        ("flagged = \"yes\"\n", "flagged"),
    ] {
        let (_directory, file) = decisions(text);
        let output = run(&["--decisions", &file], &submit("s1", None));
        assert_eq!(output.status.code(), Some(1), "{text}: {output:?}");
        assert!(output.stdout.is_empty(), "{text}: nothing runs: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(named), "{text}: names `{named}`: {stderr}");
    }
}
