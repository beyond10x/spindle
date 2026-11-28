//! Adversary of wave 33 U4 (`story:entity-invariant-over-stored-rows`), round 2, through `run`:
//! the exit code and every stdout line of a stdin run refused for broken entity invariants beside
//! a removed state, and a row a command stored without its optional field under an invariant on
//! that field.
//!
//! `tests/fixtures/adversary-w33-u4-before` keeps books with a title and an optional note,
//! shelved or lent; `tests/fixtures/adversary-w33-u4-after` removes the state `Lent` and declares
//! `title.count <= 10`, `note.count <= 4` and an invariant whose statement holds a line break.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use serde_json::{Value, json};

const BEFORE: &str = "crates/spindle-cli/tests/fixtures/adversary-w33-u4-before";
const AFTER: &str = "crates/spindle-cli/tests/fixtures/adversary-w33-u4-after";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BOOK: &str = "archive.shelf.Book";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// `run --stdin` with `spec` on the state file `file`, each of `lines` on stdin.
fn stdin_run(spec: &str, file: &Path, lines: &[Value]) -> (ExitStatus, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--stdin", "--state"])
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes());
    if let Err(error) = written {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
    }
    let output = child.wait_with_output().expect("b10x-spindle runs");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn book(identity: &str, state: &str, fields: &Value) -> Value {
    json!({"entity": BOOK, "identity": identity, "state": state, "fields": fields})
}

/// Kept: a stdin run over books that break each of three invariants once and one that rests in
/// the removed state exits 3 and prints the refusal line and exactly one line per misfit, the
/// statement holding a line break on one line, and nothing else.
#[test]
fn adv2_a_refused_stdin_run_prints_one_line_per_misfit() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let (status, stdout) = stdin_run(
        BEFORE,
        &file,
        &[json!({"arrange": [
            book("b1", "Shelved", &json!({"title": "The Left Hand of Darkness"})),
            book("b2", "Lent", &json!({"title": "Dune", "note": "signed"})),
            book("b3", "Shelved", &json!({"title": "one\ntwo"})),
        ]})],
    );
    assert!(status.success(), "{status}: {stdout}");
    let (status, stdout) = stdin_run(
        AFTER,
        &file,
        &[json!({"command": "archive.shelf.ShelveBook", "input": {"id": "b9", "title": "Emma"}})],
    );
    assert_eq!(status.code(), Some(3), "{stdout:?}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "a refusal line and four misfit lines: {stdout:?}"
    );
    assert!(lines[0].starts_with("refused: "), "{stdout:?}");
    let mut misfits: Vec<&str> = lines[1..].iter().map(|line| line.trim_start()).collect();
    misfits.sort_unstable();
    assert_eq!(
        misfits,
        vec![
            "archive.shelf.Book invariant `note.count <= 4`: the invariant does not hold; stored \
             rows breaking it: 1",
            "archive.shelf.Book invariant `title != 'one\\ntwo'`: the invariant does not hold; \
             stored rows breaking it: 1",
            "archive.shelf.Book invariant `title.count <= 10`: the invariant does not hold; \
             stored rows breaking it: 1",
            "archive.shelf.Book state Lent: the state is removed; stored rows resting in it: 1",
        ],
        "{stdout:?}"
    );
}

/// Kept: a book `ShelveBook` stored without its optional note fits `note.count <= 4` (the
/// invariant leaves it unknown), so the run under the new specification starts and answers.
#[test]
fn adv2_a_row_stored_without_its_optional_field_fits_an_invariant_on_it() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let (status, stdout) = stdin_run(
        BEFORE,
        &file,
        &[json!({"command": "archive.shelf.ShelveBook", "input": {"id": "b1", "title": "Dune"}})],
    );
    assert!(status.success(), "{status}: {stdout}");
    let (status, stdout) = stdin_run(
        AFTER,
        &file,
        &[json!({"command": "archive.shelf.ShelveBook", "input": {"id": "b2", "title": "Emma"}})],
    );
    assert!(status.success(), "{status}: {stdout:?}");
    assert_eq!(stdout.lines().count(), 1, "one answer: {stdout:?}");
    assert!(stdout.contains("shelved"), "{stdout:?}");
}
