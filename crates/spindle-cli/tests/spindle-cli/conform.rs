//! `conform`: the suite ESS synthesises from a specification, run against Spindle's in-process
//! target.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const SPEC: &str = "ess";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COUNTS: &str = "conformance.toml";
const OTHER: &str = "crates/spindle-cli/tests/fixtures/replay";

/// The scenario `ess/ess-inputs.yaml` lists under `scenarios:`, which ESS's synthesis cannot
/// write; the gate runs it.
const AUTHORED: &str = "spindle.placement/authored/launch-not-admitted";

/// Two scenarios wrong on purpose, each of which every correct implementation fails: each
/// declares an environment and then expects `EnvironmentState` to hold no row for it.
/// [`failing_spec`] lists them beside `ess/`'s own.
const WRONG: [(&str, &str); 2] = [
    (
        "declared-wrongly",
        "spindle.deployment/authored/declared-wrongly",
    ),
    (
        "declared-wrongly-again",
        "spindle.deployment/authored/declared-wrongly-again",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory")
}

fn conform(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("conform")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).expect("the report was written");
    serde_json::from_str(&text).expect("the report is JSON")
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("utf-8")
}

/// Runs `conform` on `spec` as `task check:conformance` runs it, with no known-failures
/// declaration, writing the report and the results into `dir`; returns the output, the report
/// and the results.
fn run_spec(dir: &Path, spec: &str) -> (Output, Value, Value) {
    let report = dir.join("report.json");
    let results = dir.join("results.json");
    let output = conform(&[
        spec,
        "--environment",
        LAPTOP,
        "--report",
        utf8(&report),
        "--results",
        utf8(&results),
    ]);
    (output, read_json(&report), read_json(&results))
}

/// [`run_spec`] on `ess/`.
fn run_ess(dir: &Path) -> (Output, Value, Value) {
    run_spec(dir, SPEC)
}

/// The scenario [`skipping_spec`] lists, which the in-process target skips: it takes the
/// environment's identity from a fixture, and the host provides no fixture value.
const SKIPPED: &str = "spindle.deployment/authored/fixture-provided";

/// The reason the target gives for skipping [`SKIPPED`].
const NO_FIXTURE: &str = "cannot expose fixture values: the in-process host provides no fixture";

/// A copy of `ess/` in `dir/name` whose `ess-inputs.yaml` also lists each of `scenarios`, a file
/// stem and the document written to `scenarios/<stem>.yaml`.
fn spec_with(dir: &Path, name: &str, scenarios: &[(&str, String)]) -> PathBuf {
    let spec = dir.join(name);
    copy(&root().join(SPEC), &spec);
    let manifest = spec.join("ess-inputs.yaml");
    let mut listed = std::fs::read_to_string(&manifest).expect("the manifest");
    assert!(
        listed
            .trim_end()
            .ends_with("- scenarios/launch-not-admitted.yaml"),
        "ess-inputs.yaml ends with its scenarios list: {listed}"
    );
    for (stem, document) in scenarios {
        std::fs::write(
            spec.join("scenarios").join(format!("{stem}.yaml")),
            document,
        )
        .expect("writing a scenario");
        listed.push_str(&format!("  - scenarios/{stem}.yaml\n"));
    }
    std::fs::write(&manifest, listed).expect("writing the manifest");
    spec
}

/// A copy of `ess/` in `dir` whose `ess-inputs.yaml` also lists [`SKIPPED`]: a run of it executes
/// exactly what a run of `ess/` executes, and skips one scenario more.
fn skipping_spec(dir: &Path) -> PathBuf {
    let document = "type: ess-scenario/3\n\
                    domain: spindle.deployment\n\
                    scenario: fixture-provided\n\
                    summary: An environment whose identity a fixture provides is declared.\n\
                    fixtures:\n\
                    \x20 env: spindle.deployment.EnvironmentId\n\
                    timeline:\n\
                    \x20 - at: 2026-01-05T09:00:00Z\n\
                    \x20   command: spindle.deployment.DeclareEnvironment\n\
                    \x20   input:\n\
                    \x20     environment_id: {$fixture: env}\n\
                    \x20     os: linux\n\
                    \x20     arch: x86_64\n\
                    \x20     provides: []\n\
                    \x20   outcome: declared\n";
    spec_with(
        dir,
        "skipping-ess",
        &[("fixture-provided", document.to_owned())],
    )
}

/// A copy of `ess/` in `dir` whose `ess-inputs.yaml` also lists the two [`WRONG`] scenarios.
fn failing_spec(dir: &Path) -> PathBuf {
    let wrong = WRONG.map(|(name, _)| {
        (
            name,
            format!(
                "type: ess-scenario/1\n\
                 domain: spindle.deployment\n\
                 scenario: {name}\n\
                 summary: Wrong on purpose; an environment declared is expected to be absent.\n\
                 timeline:\n\
                 \x20 - at: 2026-01-05T09:00:00Z\n\
                 \x20   command: spindle.deployment.DeclareEnvironment\n\
                 \x20   input:\n\
                 \x20     environment_id: {name}\n\
                 \x20     os: linux\n\
                 \x20     arch: x86_64\n\
                 \x20     provides: []\n\
                 \x20   outcome: declared\n\
                 assert:\n\
                 \x20 - view: spindle.deployment.EnvironmentState\n\
                 \x20   excludes: {{environment_id: {name}}}\n"
            ),
        )
    });
    spec_with(dir, "failing-ess", &wrong)
}

/// Copies the directory `from` to `to`.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("creating a directory");
    for entry in std::fs::read_dir(from).expect("reading a directory") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copying a file");
        }
    }
}

/// An `ess-known-failures/1` declaration of `scenarios` written to `dir/name`, bound to the
/// specification and suite digests `report` records and to this build, as a release writes one.
fn declaration(dir: &Path, name: &str, report: &Value, scenarios: &[&str]) -> PathBuf {
    let mut failures = scenarios
        .iter()
        .map(|scenario| {
            serde_json::json!({
                "reason": "declared by the test to see what the gate makes of it",
                "scenario": scenario,
                "tracking": "none",
            })
        })
        .collect::<Vec<_>>();
    failures.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let declaration = serde_json::json!({
        "failures": failures,
        "format": "ess-known-failures/1",
        "implementation": format!("b10x-spindle {}", env!("CARGO_PKG_VERSION")),
        "implementation_build": spindle_conformance::build(),
        "spec_digest": report["spec_digest"].as_str().expect("the spec digest"),
        "suite_digest": report["suite"]["digest"].as_str().expect("the suite digest"),
    });
    let path = dir.join(name);
    let mut text = serde_json::to_string_pretty(&declaration).expect("JSON");
    text.push('\n');
    std::fs::write(&path, text).expect("writing the declaration");
    path
}

fn scenarios(results: &Value) -> &Vec<Value> {
    results["scenarios"].as_array().expect("scenarios")
}

#[test]
fn every_executed_scenario_passes() {
    let dir = scratch();
    let (output, report, results) = run_ess(dir.path());
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let scenarios = scenarios(&results);
    let executed = scenarios
        .iter()
        .filter(|scenario| scenario["status"] != "unsupported")
        .collect::<Vec<_>>();
    assert!(!executed.is_empty(), "{results}");
    for scenario in &executed {
        assert_eq!(scenario["status"], "passed", "{scenario}");
    }
    assert!(
        executed
            .iter()
            .any(|scenario| scenario["scenario"] == AUTHORED),
        "{AUTHORED} is run and passes: {results}"
    );
    assert_eq!(report["format"], "ess-conformance-report/2", "{report}");
    assert_eq!(
        report["counts"]["total"].as_u64(),
        Some(scenarios.len() as u64),
        "{report}"
    );
    let printed = stdout(&output);
    assert!(
        printed.contains(&format!(
            "{} scenario(s): {} executed, {} passed, 0 failed, 0 error, {} skipped",
            scenarios.len(),
            executed.len(),
            executed.len(),
            scenarios.len() - executed.len()
        )),
        "{printed}"
    );
}

/// Every scenario a run skips is printed, reported and listed with the target's reason: on the
/// specification with [`SKIPPED`], which the run does skip.
#[test]
fn every_skipped_scenario_has_a_reason() {
    let dir = scratch();
    let skipping = skipping_spec(dir.path());
    let (output, report, results) = run_spec(dir.path(), utf8(&skipping));
    let printed = stdout(&output);
    let skipped = scenarios(&results)
        .iter()
        .filter(|scenario| scenario["status"] == "unsupported")
        .collect::<Vec<_>>();
    assert_eq!(
        skipped
            .iter()
            .map(|scenario| scenario["scenario"].as_str())
            .collect::<Vec<_>>(),
        vec![Some(SKIPPED)],
        "the run skips {SKIPPED} and nothing else: {results}"
    );
    assert!(
        printed.contains(&format!("skipped {SKIPPED}: {NO_FIXTURE}")),
        "{printed}"
    );
    for scenario in &skipped {
        let id = scenario["scenario"].as_str().expect("an id");
        let reasons = scenario["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .filter(|check| check["status"] == "unsupported")
            .filter_map(|check| check["diagnostic"]["observed"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .collect::<Vec<_>>();
        assert!(
            !reasons.is_empty(),
            "{id} is skipped with no reason: {scenario}"
        );
        assert!(
            printed.contains(&format!("skipped {id}: {}", reasons[0])),
            "{id} is printed with its reason: {printed}"
        );
        assert!(
            report["outcomes"]["unsupported"]
                .as_array()
                .expect("the unsupported outcomes")
                .iter()
                .any(|listed| listed.as_str() == Some(id)),
            "{id} in {report}"
        );
    }
}

/// The lines of the task `name` in a Taskfile: from its key to the next task's.
fn task<'a>(taskfile: &'a str, name: &str) -> Vec<&'a str> {
    let key = format!("  {name}:");
    taskfile
        .lines()
        .skip_while(|line| *line != key)
        .skip(1)
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .collect()
}

#[test]
fn the_synthesis_refusals_are_listed() {
    let output = conform(&[SPEC, "--environment", LAPTOP]);
    let printed = stdout(&output);
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("ess/ loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let listed = printed
        .lines()
        .filter(|line| line.starts_with("refused: refusal["))
        .count();
    assert_eq!(listed, synthesis.refusals.len(), "{printed}");
    for refusal in &synthesis.refusals {
        let first = refusal.to_string();
        let first = first.lines().next().expect("a refusal line");
        assert!(
            first.contains(&format!("refusal[{}]", refusal.code())),
            "{first}"
        );
        assert!(
            printed.contains(&format!("refused: {first}")),
            "{first} is printed: {printed}"
        );
    }
    assert!(
        printed.contains(&format!("{} refusal(s)", synthesis.refusals.len())),
        "{printed}"
    );
}

#[test]
fn the_target_runs_a_suite_of_another_specification() {
    let dir = scratch();
    let loaded = spindle_spec::load(&root().join(OTHER)).expect("the fixture loads");
    let mut synthesis = ess_conformance::synthesize(&loaded.ir);
    synthesis.suite.select_fresh_format_for(&loaded.ir);
    let total = synthesis.suite.len();
    assert!(total > 0);
    let suite = dir.path().join("suite.json");
    std::fs::write(
        &suite,
        synthesis
            .suite
            .to_canonical_json()
            .expect("the suite renders"),
    )
    .expect("writing the suite");
    let report = dir.path().join("report.json");
    let output = conform(&[
        OTHER,
        "--environment",
        LAPTOP,
        "--suite",
        suite.to_str().expect("utf-8"),
        "--report",
        report.to_str().expect("utf-8"),
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let report = read_json(&report);
    assert_eq!(
        report["counts"]["total"].as_u64(),
        Some(total as u64),
        "{report}"
    );
    assert_eq!(report["specification"], "desk/v1", "{report}");
    assert_eq!(report["execution_status"], "passed", "{report}");

    let foreign = conform(&[
        SPEC,
        "--environment",
        LAPTOP,
        "--suite",
        suite.to_str().expect("utf-8"),
    ]);
    assert_eq!(foreign.status.code(), Some(1), "{foreign:?}");
}

/// Runs `conform` on `spec` as `task check:conformance` does, holding the counts in `counts`;
/// with `known`, reading that declaration and writing its accounting into `dir`.
fn gate(dir: &Path, spec: &str, counts: &str, known: Option<&Path>) -> Output {
    let report = dir.join("gate-report.json");
    let accounting = dir.join("gate-accounting.json");
    let mut arguments = vec![
        spec,
        "--environment",
        LAPTOP,
        "--counts",
        counts,
        "--report",
        utf8(&report),
    ];
    if let Some(known) = known {
        arguments.extend(["--known-failing", utf8(known)]);
        arguments.extend(["--accounting-out", utf8(&accounting)]);
    }
    conform(&arguments)
}

/// A counts document holding the scenarios in `executed`.
fn counts(executed: &[String], skipped: i64, known_failed: i64) -> String {
    let mut table = toml::Table::new();
    table.insert(
        "format".to_owned(),
        toml::Value::from("spindle-conformance-counts/1"),
    );
    table.insert(
        "executed_scenarios".to_owned(),
        toml::Value::Array(
            executed
                .iter()
                .map(|id| toml::Value::from(id.as_str()))
                .collect(),
        ),
    );
    table.insert("skipped".to_owned(), toml::Value::from(skipped));
    table.insert("known_failed".to_owned(), toml::Value::from(known_failed));
    toml::to_string(&table).expect("the counts render")
}

/// The committed counts: the executed scenarios, the skipped and the known-failed counts.
fn committed() -> (Vec<String>, i64, i64) {
    let committed = std::fs::read_to_string(root().join(COUNTS)).expect("the committed counts");
    let table: toml::Table = committed.parse().expect("the counts are TOML");
    let executed = table["executed_scenarios"]
        .as_array()
        .expect("executed_scenarios")
        .iter()
        .map(|id| id.as_str().expect("an id").to_owned())
        .collect();
    (
        executed,
        table["skipped"].as_integer().expect("skipped"),
        table["known_failed"].as_integer().expect("known_failed"),
    )
}

/// Writes `text` to `dir/name` as a counts document; returns its path.
fn counts_file(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("writing counts");
    path
}

/// Runs the gate on `ess/` against `text` as the counts document.
fn gate_with(dir: &Path, name: &str, text: &str) -> Output {
    let path = counts_file(dir, name, text);
    gate(dir, SPEC, utf8(&path), None)
}

/// The committed executed set with the [`WRONG`] scenarios added: what the gate on
/// [`failing_spec`] executes.
fn failing_executed() -> Vec<String> {
    let (mut executed, _, _) = committed();
    executed.extend(WRONG.iter().map(|(_, id)| (*id).to_owned()));
    executed.sort();
    executed
}

/// The report of a run of `spec`, for the digests a declaration binds.
fn report_of(dir: &Path, spec: &str) -> Value {
    let run = dir.join("digests");
    std::fs::create_dir_all(&run).expect("a directory for the run");
    run_spec(&run, spec).1
}

#[test]
fn the_suite_counts_are_held() {
    let dir = scratch();
    let held = gate(dir.path(), SPEC, COUNTS, None);
    assert_eq!(held.status.code(), Some(0), "{held:?}");
    assert!(
        stdout(&held).contains(&format!("counts held against {COUNTS}")),
        "{held:?}"
    );
    assert!(
        !stdout(&held).contains("the counts moved"),
        "the committed counts are exactly the run's: {held:?}"
    );

    let (executed, skipped, known_failed) = committed();
    assert_eq!(known_failed, 0, "the repository declares no known failure");
    assert!(
        executed.iter().any(|id| id == AUTHORED),
        "{AUTHORED} is held as executed"
    );

    assert_eq!(
        skipped, 0,
        "nothing is skipped; a_skipped_scenario_fails_the_committed_counts holds the skipped count"
    );

    // Known failures are held too: on a specification with two scenarios that fail, both
    // declared, counts that hold no known failure fail the gate, and counts that hold both pass.
    let failing = failing_spec(dir.path());
    let failing = utf8(&failing);
    let known = declaration(
        dir.path(),
        "both.json",
        &report_of(dir.path(), failing),
        &WRONG.map(|(_, id)| id),
    );
    let none = counts_file(
        dir.path(),
        "none-known.toml",
        &counts(&failing_executed(), 0, 0),
    );
    let grown = gate(dir.path(), failing, utf8(&none), Some(&known));
    assert_eq!(grown.status.code(), Some(1), "{grown:?}");
    assert!(
        stdout(&grown).contains("known failures grew from 0 to 2"),
        "{grown:?}"
    );
    let both = counts_file(
        dir.path(),
        "both-known.toml",
        &counts(&failing_executed(), 0, 2),
    );
    let held = gate(dir.path(), failing, utf8(&both), Some(&known));
    assert_eq!(held.status.code(), Some(0), "{held:?}");

    let taskfile = std::fs::read_to_string(root().join("Taskfile.yml")).expect("Taskfile.yml");
    let check = task(&taskfile, "check");
    assert!(
        check
            .iter()
            .any(|line| line.trim() == "- task: check:conformance"),
        "task check runs check:conformance: {check:?}"
    );
    let steps = task(&taskfile, "check:conformance");
    assert!(
        steps
            .iter()
            .any(|line| line.contains(" conform synthesize ")
                && line.contains(&format!("--path {SPEC}"))
                && line.contains(&format!("--scenarios {SPEC}"))),
        "check:conformance synthesises the suite with ess/'s authored scenarios: {steps:?}"
    );
    assert!(
        steps.iter().any(|line| line.contains(" conform ess ")
            && line.contains(&format!("--counts {COUNTS}"))
            && line.contains("--suite ")),
        "check:conformance runs that suite and holds the committed counts: {steps:?}"
    );
    assert!(
        steps.iter().all(|line| !line.contains("--known-failing")),
        "check:conformance declares no known failure: {steps:?}"
    );
}

/// The skipped count is held: a run that executes exactly the committed set and skips one
/// scenario more fails the gate against the committed counts, naming the scenario and the count,
/// and passes against counts that hold the skip.
#[test]
fn a_skipped_scenario_fails_the_committed_counts() {
    let dir = scratch();
    let skipping = skipping_spec(dir.path());
    let skipping = utf8(&skipping);

    let grown = gate(dir.path(), skipping, COUNTS, None);
    assert_eq!(grown.status.code(), Some(1), "{grown:?}");
    let printed = stdout(&grown);
    assert!(
        printed.contains(&format!("skipped {SKIPPED}: {NO_FIXTURE}")),
        "{printed}"
    );
    assert!(
        printed.contains(&format!("{COUNTS}: skipped grew from 0 to 1")),
        "{printed}"
    );
    assert!(
        !printed.contains("no longer executes") && !printed.contains("is not held"),
        "the executed set is the committed one, so only the skipped count fails: {printed}"
    );

    let (executed, _, _) = committed();
    let held = counts_file(dir.path(), "skipped-one.toml", &counts(&executed, 1, 0));
    let held = gate(dir.path(), skipping, utf8(&held), None);
    assert_eq!(held.status.code(), Some(0), "{held:?}");
}

/// The gate goes red when a declared scenario passes: a declaration of the two scenarios that
/// fail with a scenario that passes declared beside them, bound to the same suite, specification
/// and build.
#[test]
fn a_declared_scenario_that_passes_fails_the_gate() {
    const PASSING: &str = "spindle.deployment.DeclareEnvironment/outcome/declared";
    let dir = scratch();
    let failing = failing_spec(dir.path());
    let failing = utf8(&failing);
    let stale = declaration(
        dir.path(),
        "stale.json",
        &report_of(dir.path(), failing),
        &[WRONG[0].1, WRONG[1].1, PASSING],
    );
    let counts = counts_file(
        dir.path(),
        "counts.toml",
        &counts(&failing_executed(), 0, 2),
    );

    let output = gate(dir.path(), failing, utf8(&counts), Some(&stale));
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("known-failures.stale") && printed.contains(PASSING),
        "{printed}"
    );
}

/// The gate goes red when a scenario fails undeclared: the run of the failing specification
/// without a declaration, as the task runs.
#[test]
fn an_undeclared_failure_fails_the_gate() {
    let dir = scratch();
    let failing = failing_spec(dir.path());
    let counts = counts_file(
        dir.path(),
        "counts.toml",
        &counts(&failing_executed(), 0, 0),
    );
    let output = gate(dir.path(), utf8(&failing), utf8(&counts), None);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    for (_, id) in WRONG {
        assert!(
            stdout(&output).contains(&format!("failed {id}")),
            "{output:?}"
        );
    }
}

/// The gate goes red on a second failure despite a declaration: one of the two failing
/// scenarios declared, the other is accounted unexpected.
#[test]
fn a_second_failure_fails_the_gate_despite_the_declaration() {
    let dir = scratch();
    let failing = failing_spec(dir.path());
    let failing = utf8(&failing);
    let one = declaration(
        dir.path(),
        "one.json",
        &report_of(dir.path(), failing),
        &[WRONG[0].1],
    );
    let counts = counts_file(
        dir.path(),
        "counts.toml",
        &counts(&failing_executed(), 0, 1),
    );
    let output = gate(dir.path(), failing, utf8(&counts), Some(&one));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let accounting = read_json(&dir.path().join("gate-accounting.json"));
    assert_eq!(
        accounting["unexpected_failed"],
        serde_json::json!([WRONG[1].1]),
        "{accounting}"
    );
    assert_eq!(
        accounting["known_failed"].as_array().map(Vec::len),
        Some(1),
        "{accounting}"
    );
}

/// The executed scenarios are held as a set, not a total: a scenario that stops executing while
/// another starts, which leaves every total unchanged, fails the gate naming both.
#[test]
fn the_executed_set_is_held() {
    let dir = scratch();
    let (executed, skipped, known_failed) = committed();
    // A scenario of another suite, which a run of `ess/` does not execute.
    let absent = WRONG[0].1.to_owned();
    let Some(dropped) = executed.first() else {
        panic!("the committed counts hold an executed scenario");
    };
    assert!(!executed.contains(&absent));

    let mut listed = executed.clone();
    listed.push(absent.clone());
    listed.sort();
    let lost = gate_with(
        dir.path(),
        "lost.toml",
        &counts(&listed, skipped, known_failed),
    );
    assert_eq!(lost.status.code(), Some(1), "{lost:?}");
    assert!(
        stdout(&lost).contains(&format!(
            "`{absent}` is held as executed and no longer executes"
        )),
        "{lost:?}"
    );

    let unlisted = executed[1..].to_vec();
    let grown = gate_with(
        dir.path(),
        "grown.toml",
        &counts(&unlisted, skipped, known_failed),
    );
    assert_eq!(grown.status.code(), Some(1), "{grown:?}");
    assert!(
        stdout(&grown).contains(&format!(
            "`{dropped}` executes and is not held; add it to executed_scenarios"
        )),
        "{grown:?}"
    );

    let mut swapped = executed[1..].to_vec();
    swapped.push(absent.clone());
    swapped.sort();
    assert_eq!(swapped.len(), executed.len());
    let swap = gate_with(
        dir.path(),
        "swap.toml",
        &counts(&swapped, skipped, known_failed),
    );
    assert_eq!(swap.status.code(), Some(1), "{swap:?}");
    let printed = stdout(&swap);
    assert!(
        printed.contains(&format!(
            "`{absent}` is held as executed and no longer executes"
        )) && printed.contains(&format!("`{dropped}` executes and is not held")),
        "{printed}"
    );
}
