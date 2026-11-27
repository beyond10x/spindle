//! Adversary cases for wave 30 U5, story:create-on-held-identity-unavailable: what `run
//! --scenario` and a stdin `run` print for a create on a held identity.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, AdvancingClock, ConformanceSuite, Ids, Runner, RunnerConfig, Status,
    SuiteProvenance,
};
use serde_json::Value;

const LAPTOP: &str = "examples/environments/laptop.toml";
const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-w30-u5";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str], lines: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(if lines.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    if let Some(lines) = lines {
        let written = child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(lines.as_bytes());
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
        }
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

fn spec() -> PathBuf {
    root().join(SPEC)
}

fn laptop() -> PathBuf {
    root().join(LAPTOP)
}

fn replay(scenario: &Path) -> Output {
    spindle(
        &[
            "run",
            spec().to_str().expect("utf-8 path"),
            "--environment",
            laptop().to_str().expect("utf-8 path"),
            "--scenario",
            scenario.to_str().expect("utf-8 path"),
        ],
        None,
    )
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn scenario(name: &str, text: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let path = directory.path().join(name);
    std::fs::write(&path, text).expect("written");
    (directory, path)
}

/// The status ESS 0.99.2's own runner gives `document` against ESS's interpreter of [`SPEC`].
fn ess_runner_status(document: &str) -> Status {
    let ir = spindle_spec::load(&spec())
        .expect("the specification compiles")
        .ir;
    let authoring = authored::compile(
        &ir,
        &[Source::new("scenario.yaml".to_owned(), document.to_owned())],
    );
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(&ir));
    for (id, compiled) in authoring.scenarios {
        suite.insert(id, compiled).expect("one scenario");
    }
    suite.select_fresh_format_for(&ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let target = Interpreted::for_model(ir.clone());
    Runner::new(
        RunnerConfig::default(),
        AdvancingClock::default(),
        Ids::for_suite(&suite),
    )
    .run_admitted(&admitted, &target)
    .into_report()
    .scenarios[0]
        .status
}

/// A replay whose second act takes the guarded one of two creating outcomes on the identity the
/// first act created: the divergent line ends `, unavailable`, and the check printed under it
/// names that guarded outcome in ESS's words.
#[test]
fn w30_u5_a_replay_diverging_on_a_guarded_held_create_names_that_outcome() {
    let document = "type: ess-scenario/1\n\
         domain: shelf.box\n\
         scenario: a-box-packed-twice\n\
         summary: A box is packed, then packed again in a rush.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: shelf.box.PackBox\n\
         \x20   input: {box_id: b1, label: first, rush: false}\n\
         \x20   outcome: packed\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: shelf.box.PackBox\n\
         \x20   input: {box_id: b1, label: second, rush: true}\n\
         \x20   outcome: rushed\n";
    assert_eq!(ess_runner_status(document), Status::Error);
    let (_directory, path) = scenario("twice.yaml", document);
    let output = replay(&path);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    for line in [
        "timeline[0] shelf.box.PackBox: holds, outcome packed\n",
        "timeline[1] shelf.box.PackBox: diverges, unavailable\n",
        "`shelf.box.PackBox/rushed`: a supplied identity is already held by `shelf.box.Box`, and \
         creation never replaces it",
    ] {
        assert!(printed.contains(line), "{line:?} in {printed}");
    }
}

/// A scenario that establishes a tally under the `Integer` identity 7 and then opens a tally 7:
/// ESS's runner fails it, the create on the held identity answered unavailable; the replay on
/// the host diverges on that act as well.
#[test]
fn w30_u5_a_replay_of_a_create_on_an_established_integer_identity_diverges() {
    let document = "type: ess-scenario/2\n\
         domain: shelf.box\n\
         scenario: a-tally-opened-on-an-established-one\n\
         summary: A tally established as 7 is opened again as 7.\n\
         arrange:\n\
         \x20 - instance: seven\n\
         \x20   entity: shelf.box.Tally\n\
         \x20   setup:\n\
         \x20     identity: 7\n\
         \x20     fields: {label: established}\n\
         \x20     state: Open\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: shelf.box.OpenTally\n\
         \x20   input: {tally_id: 7, label: again}\n\
         \x20   outcome: opened\n";
    assert_ne!(ess_runner_status(document), Status::Passed);
    let (_directory, path) = scenario("seven.yaml", document);
    let output = replay(&path);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        printed.contains("timeline[0] shelf.box.OpenTally: diverges, unavailable\n"),
        "{printed}"
    );
}

/// A stdin `run` of two creates of one identity: the second line's failure keeps the kernel's
/// kind and message, as the story's safety fact states; only the conformance target's answer
/// moves.
#[test]
fn w30_u5_a_stdin_create_on_a_held_identity_keeps_its_failure() {
    let output = spindle(
        &[
            "run",
            spec().to_str().expect("utf-8 path"),
            "--environment",
            laptop().to_str().expect("utf-8 path"),
        ],
        Some(
            "{\"command\": \"shelf.box.PackBox\", \"input\": {\"box_id\": \"b1\", \"label\": \
             \"first\", \"rush\": false}}\n\
             {\"command\": \"shelf.box.PackBox\", \"input\": {\"box_id\": \"b1\", \"label\": \
             \"second\", \"rush\": true}}\n",
        ),
    );
    let printed = stdout(&output);
    let lines = printed
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a JSON line"))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2, "{output:?}");
    assert_eq!(lines[0]["outcome"], "packed", "{printed}");
    assert_eq!(
        lines[1]["failure"],
        serde_json::json!({
            "kind": "undeclared",
            "message": "shelf.box.PackBox: a `shelf.box.Box` already has the identity \"b1\", \
                        and `creates:` never replaces an instance",
        }),
        "{printed}"
    );
}
