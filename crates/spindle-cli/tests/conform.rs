//! `conform`: the suite ESS synthesises from a specification, run against Spindle's in-process
//! target.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const SPEC: &str = "ess";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COUNTS: &str = "conformance.toml";
const KNOWN: &str = "conformance-known-failures.json";
const OTHER: &str = "crates/spindle-cli/tests/fixtures/replay";

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

/// Runs `conform` on `ess/` with the committed known-failures declaration, writing the reports and
/// the accounting into `dir`; returns the output, the report, the results and the accounting.
fn run_ess(dir: &Path) -> (Output, Value, Value, Value) {
    let report = dir.join("report.json");
    let results = dir.join("results.json");
    let accounting = dir.join("accounting.json");
    let output = conform(&[
        SPEC,
        "--environment",
        LAPTOP,
        "--report",
        report.to_str().expect("utf-8"),
        "--results",
        results.to_str().expect("utf-8"),
        "--known-failing",
        KNOWN,
        "--accounting-out",
        accounting.to_str().expect("utf-8"),
    ]);
    (
        output,
        read_json(&report),
        read_json(&results),
        read_json(&accounting),
    )
}

/// The scenarios the committed declaration names.
fn declared() -> Vec<String> {
    read_json(&root().join(KNOWN))["failures"]
        .as_array()
        .expect("failures")
        .iter()
        .map(|failure| failure["scenario"].as_str().expect("an id").to_owned())
        .collect()
}

fn scenarios(results: &Value) -> &Vec<Value> {
    results["scenarios"].as_array().expect("scenarios")
}

#[test]
fn every_executed_scenario_passes() {
    let dir = scratch();
    let (output, report, results, accounting) = run_ess(dir.path());
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let scenarios = scenarios(&results);
    let executed = scenarios
        .iter()
        .filter(|scenario| scenario["status"] != "unsupported")
        .collect::<Vec<_>>();
    assert!(!executed.is_empty(), "{results}");
    let declared = declared();
    for scenario in &executed {
        let id = scenario["scenario"].as_str().expect("an id");
        let expected = if declared.iter().any(|known| known == id) {
            "failed"
        } else {
            "passed"
        };
        assert_eq!(scenario["status"], expected, "{scenario}");
    }
    assert_eq!(
        accounting["unexpected_failed"].as_array().map(Vec::len),
        Some(0),
        "{accounting}"
    );
    assert_eq!(
        accounting["known_failed"].as_array().map(Vec::len),
        Some(declared.len()),
        "{accounting}"
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
            "{} scenario(s): {} executed, {} passed, {} failed, 0 error, {} skipped",
            scenarios.len(),
            executed.len(),
            executed.len() - declared.len(),
            declared.len(),
            scenarios.len() - executed.len()
        )),
        "{printed}"
    );
}

#[test]
fn every_skipped_scenario_has_a_reason() {
    let dir = scratch();
    let (output, report, results, _) = run_ess(dir.path());
    let printed = stdout(&output);
    let skipped = scenarios(&results)
        .iter()
        .filter(|scenario| scenario["status"] == "unsupported")
        .collect::<Vec<_>>();
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
    let synthesis = ess_conformance::synthesize(&loaded.ir);
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

/// Runs `conform` on `ess/` as `task check:conformance` does, holding the counts in `counts` and
/// reading the declaration `known`.
fn gate(dir: &Path, counts: &str, known: &str) -> Output {
    let report = dir.join("gate-report.json");
    let accounting = dir.join("gate-accounting.json");
    conform(&[
        SPEC,
        "--environment",
        LAPTOP,
        "--counts",
        counts,
        "--report",
        report.to_str().expect("utf-8"),
        "--known-failing",
        known,
        "--accounting-out",
        accounting.to_str().expect("utf-8"),
    ])
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

/// Runs the gate against `text` as the counts document.
fn gate_with(dir: &Path, name: &str, text: &str) -> Output {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("writing counts");
    gate(dir, path.to_str().expect("utf-8"), KNOWN)
}

#[test]
fn the_suite_counts_are_held() {
    let dir = scratch();
    let held = gate(dir.path(), COUNTS, KNOWN);
    assert_eq!(held.status.code(), Some(0), "{held:?}");
    assert!(
        stdout(&held).contains(&format!("counts held against {COUNTS}")),
        "{held:?}"
    );

    let (executed, skipped, known_failed) = committed();
    assert_eq!(known_failed, declared().len() as i64);

    if skipped > 0 {
        let grown = gate_with(
            dir.path(),
            "fewer.toml",
            &counts(&executed, skipped - 1, known_failed),
        );
        assert_eq!(grown.status.code(), Some(1), "{grown:?}");
        assert!(
            stdout(&grown).contains(&format!("skipped grew from {} to {skipped}", skipped - 1)),
            "{grown:?}"
        );
    }

    if known_failed > 0 {
        let grown = gate_with(
            dir.path(),
            "fewer-known.toml",
            &counts(&executed, skipped, known_failed - 1),
        );
        assert_eq!(grown.status.code(), Some(1), "{grown:?}");
        assert!(
            stdout(&grown).contains(&format!(
                "known failures grew from {} to {known_failed}",
                known_failed - 1
            )),
            "{grown:?}"
        );
    }

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
        steps.iter().any(|line| line.contains(" conform ess ")
            && line.contains(&format!("--counts {COUNTS}"))
            && line.contains(&format!("--known-failing {KNOWN}"))),
        "check:conformance holds the committed counts and declaration: {steps:?}"
    );
}

/// The gate goes red when a declared scenario passes: the committed declaration with a scenario
/// that passes declared beside its own, bound to the same suite, specification and build.
#[test]
fn a_declared_scenario_that_passes_fails_the_gate() {
    const PASSING: &str = "spindle.deployment.DeclareEnvironment/outcome/declared";
    let dir = scratch();
    let mut declaration = read_json(&root().join(KNOWN));
    let failures = declaration["failures"].as_array_mut().expect("failures");
    failures.push(serde_json::json!({
        "reason": "declared to see the gate refuse a stale declaration",
        "scenario": PASSING,
        "tracking": "none",
    }));
    failures.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let stale = dir.path().join("stale.json");
    let mut text = serde_json::to_string_pretty(&declaration).expect("JSON");
    text.push('\n');
    std::fs::write(&stale, text).expect("writing the declaration");

    let output = gate(dir.path(), COUNTS, stale.to_str().expect("utf-8"));
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let printed = stdout(&output);
    assert!(
        printed.contains("known-failures.stale") && printed.contains(PASSING),
        "{printed}"
    );
}

/// The gate goes red when a declared scenario fails undeclared: the run without the declaration.
#[test]
fn an_undeclared_failure_fails_the_gate() {
    let output = conform(&[SPEC, "--environment", LAPTOP]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    for known in declared() {
        assert!(
            stdout(&output).contains(&format!("failed {known}")),
            "{output:?}"
        );
    }
}

/// The executed scenarios are held as a set, not a total: a scenario that stops executing while
/// another starts, which leaves every total unchanged, fails the gate naming both.
#[test]
fn the_executed_set_is_held() {
    let dir = scratch();
    let (executed, skipped, known_failed) = committed();
    let (_, report, _, _) = run_ess(dir.path());
    let unsupported = report["outcomes"]["unsupported"]
        .as_array()
        .expect("the unsupported outcomes")
        .iter()
        .map(|id| id.as_str().expect("an id").to_owned())
        .collect::<Vec<_>>();
    let (Some(now_skipped), Some(dropped)) = (unsupported.first(), executed.first()) else {
        panic!("the run skips a scenario and executes one: {report}");
    };

    let mut listed = executed.clone();
    listed.push(now_skipped.clone());
    listed.sort();
    let lost = gate_with(
        dir.path(),
        "lost.toml",
        &counts(&listed, skipped, known_failed),
    );
    assert_eq!(lost.status.code(), Some(1), "{lost:?}");
    assert!(
        stdout(&lost).contains(&format!(
            "`{now_skipped}` is held as executed and no longer executes"
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
    swapped.push(now_skipped.clone());
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
            "`{now_skipped}` is held as executed and no longer executes"
        )) && printed.contains(&format!("`{dropped}` executes and is not held")),
        "{printed}"
    );
}
