//! Adversary cases for `story:environment-binds-external-decisions` (wave 28, unit U6): an
//! environment binds external causes to `decisions-file` and names the file in a `[decisions]`
//! table; `run` refuses an external implementation it does not provide.
//!
//! The fixture `tests/fixtures/review` declares `probe.review.Submit`, whose external outcomes
//! `escalated` and `flagged` sit beside a default branch, and `probe.review.Decide`, whose external
//! outcome `vetoed` has none. `tests/fixtures/served-review` declares the same, reached over the
//! network.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/review";
const SERVED_SPEC: &str = "crates/spindle-cli/tests/fixtures/served-review";
const LAPTOP: &str = "examples/environments/laptop.toml";
/// How long a refused served `run` may take to exit on its own.
const EXIT: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A stdin `run` of the review fixture on `environment`, started in `directory`.
fn run_in(directory: &Path, environment: &Path, extra: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(directory)
        .arg("run")
        .arg(root().join(SPEC))
        .arg("--environment")
        .arg(environment)
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    // A run refused at start exits before it reads stdin; its closed pipe is that refusal, which
    // the caller reads from the exit code and stderr.
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    if let Err(error) = written {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

/// A memory-store environment whose `[provides]` ends with `external` (the `external.` keys) and
/// then holds `extra`, written to `path`.
fn environment_at(path: &Path, external: &str, extra: &str) -> PathBuf {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the environment's directory");
    }
    std::fs::write(
        path,
        format!(
            "format = \"spindle-environment/1\"\nname = \"scratchpad\"\nos = \"linux\"\n\
             arch = \"x86_64\"\n\n[provides]\n\"store.durable\" = [\"memory\"]\n{external}\n{extra}"
        ),
    )
    .expect("the environment is written");
    path.to_owned()
}

fn results(output: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn submit(id: &str) -> String {
    format!(
        r#"{{"command": "probe.review.Submit", "input": {{"submission_id": "{id}", "amount": 5}}}}"#
    ) + "\n"
}

fn decide(id: &str) -> String {
    format!(
        r#"{{"command": "probe.review.Decide", "input": {{"decision_id": "{id}", "approve": true}}}}"#
    ) + "\n"
}

/// The documents (`environment-and-admission.md` § External causes answered from a decisions file,
/// the `external.*` row, the changelog) say the environment's file answers each cause bound to
/// `decisions-file`, and a cause bound to `operator-prompt` is answered from the line,
/// `--decisions` or `--answer-default`. With `Decide.vetoed` alone bound to `decisions-file`,
/// `Submit`'s causes stay bound to `operator-prompt`: the environment's file must not answer them.
#[test]
fn a_cause_bound_to_operator_prompt_is_not_answered_from_the_environment_s_file() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    std::fs::write(
        directory.path().join("answers.toml"),
        "[\"probe.review.Submit\"]\nflagged = true\n\n[probe.review.Decide]\nvetoed = false\n",
    )
    .expect("the decisions file is written");
    let environment = environment_at(
        &directory.path().join("environment.toml"),
        "\"external.*\" = [\"operator-prompt\"]\n\
         \"external.probe.review.Decide.vetoed\" = [\"decisions-file\"]\n",
        "[decisions]\nfile = \"answers.toml\"\n",
    );
    let output = run_in(
        directory.path(),
        &environment,
        &[],
        &(submit("s1") + &decide("d1")),
    );
    let results = results(&output);
    assert_eq!(results.len(), 2, "{output:?}");
    assert_eq!(
        results[1]["outcome"], "approved",
        "the cause bound to decisions-file is answered from the file: {results:?}"
    );
    assert_eq!(
        results[0]["failure"]["kind"], "undecided",
        "Submit's causes are bound to operator-prompt, and neither the line, --decisions nor \
         --answer-default answers them: {results:?}"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// D2: `file` is relative to where `run` starts, not to the environment document. The unit's
/// own test starts `run` in the document's directory, where both readings name one file.
#[test]
fn the_environment_s_file_is_read_relative_to_where_run_starts() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    for (place, answers) in [
        ("answers", "flagged = false\nescalated = true\n"),
        ("documents/answers", "flagged = true\nescalated = false\n"),
    ] {
        std::fs::create_dir_all(directory.path().join(place)).expect("a directory for the file");
        std::fs::write(
            directory.path().join(place).join("review.toml"),
            format!("[\"probe.review.Submit\"]\n{answers}"),
        )
        .expect("a decisions file is written");
    }
    let environment = environment_at(
        &directory.path().join("documents/environment.toml"),
        "\"external.*\" = [\"decisions-file\"]\n",
        "[decisions]\nfile = \"answers/review.toml\"\n",
    );
    let output = run_in(directory.path(), &environment, &[], &submit("s1"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        results(&output)[0]["outcome"],
        "escalated",
        "the file beside where run starts answers, not the one beside the document: {output:?}"
    );
}

/// D1: the refusal looks at every external binding, the last one included (`Submit.flagged`
/// sorts after `Decide.vetoed` and `Submit.escalated`).
#[test]
fn an_unprovided_implementation_on_the_last_external_cause_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment_at(
        &directory.path().join("environment.toml"),
        "\"external.*\" = [\"operator-prompt\"]\n\
         \"external.probe.review.Submit.flagged\" = [\"desk-phone\"]\n",
        "",
    );
    let output = run_in(
        directory.path(),
        &environment,
        &["--answer-default"],
        &submit("s1"),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "nothing runs: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("external.probe.review.Submit.flagged") && stderr.contains("desk-phone"),
        "names the capability and the name: {stderr}"
    );
}

/// D3 on one exact key: the cause bound to `decisions-file` with no file named is the last
/// external binding, the others bound to `operator-prompt`.
#[test]
fn one_cause_bound_to_decisions_file_without_a_file_exits_1_naming_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment_at(
        &directory.path().join("environment.toml"),
        "\"external.*\" = [\"operator-prompt\"]\n\
         \"external.probe.review.Submit.flagged\" = [\"decisions-file\"]\n",
        "",
    );
    let output = run_in(
        directory.path(),
        &environment,
        &["--answer-default"],
        &submit("s1"),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "nothing runs: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("external.probe.review.Submit.flagged")
            && stderr.contains("decisions-file"),
        "names the capability and the implementation: {stderr}"
    );
}

/// A `[decisions]` file that is not there stops `run` before anything runs, naming the file.
#[test]
fn a_missing_environment_decisions_file_exits_1_before_running_naming_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment_at(
        &directory.path().join("environment.toml"),
        "\"external.*\" = [\"decisions-file\"]\n",
        "[decisions]\nfile = \"absent-answers.toml\"\n",
    );
    let output = run_in(
        directory.path(),
        &environment,
        &["--answer-default"],
        &submit("s1"),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "nothing runs: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("absent-answers.toml"),
        "names the file: {stderr}"
    );
}

/// A served `run` of the served fixture on the laptop environment with `"external.*"` bound to
/// `external` and `extra` appended, which is expected to exit on its own within [`EXIT`].
fn served_refused(external: &str, extra: &str) -> Output {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let laptop = std::fs::read_to_string(root().join(LAPTOP)).expect("the laptop environment");
    assert!(laptop.contains("\"external.*\" = [\"operator-prompt\"]"));
    let environment = directory.path().join("environment.toml");
    std::fs::write(
        &environment,
        format!(
            "{}\n{extra}",
            laptop.replace(
                "\"external.*\" = [\"operator-prompt\"]",
                &format!("\"external.*\" = [\"{external}\"]")
            )
        ),
    )
    .expect("the environment is written");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(directory.path())
        .arg("run")
        .arg(root().join(SERVED_SPEC))
        .arg("--environment")
        .arg(&environment)
        .arg("--state")
        .arg(directory.path().join("state"))
        .arg("--answer-default")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let started = Instant::now();
    while child.try_wait().expect("the child is waited on").is_none() {
        if started.elapsed() > EXIT {
            let _ = child.kill();
            let mut stdout = String::new();
            if let Some(mut piped) = child.stdout.take() {
                let _ = piped.read_to_string(&mut stdout);
            }
            let _ = child.wait();
            panic!("run did not exit within {EXIT:?}; it printed {stdout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("the output is read")
}

/// D1 on a served run: an unprovided implementation exits 1 before listening.
#[test]
fn a_served_run_refuses_an_unprovided_implementation_before_listening() {
    let output = served_refused("operator", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("listening"),
        "nothing listens: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("external.probe.review.") && stderr.contains("`operator`"),
        "names the capability and the name: {stderr}"
    );
}

/// D3 on a served run: `decisions-file` bound with no file named exits 1 before listening, even
/// with `--answer-default`.
#[test]
fn a_served_run_bound_to_decisions_file_without_a_file_exits_1_before_listening() {
    let output = served_refused("decisions-file", "");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("listening"),
        "nothing listens: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("external.probe.review.") && stderr.contains("decisions-file"),
        "names the capability and the implementation: {stderr}"
    );
}
