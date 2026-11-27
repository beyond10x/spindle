//! Adversary of wave 30 U9 (story:rally-example-reads-stdin): `run --stdin` reads JSON lines where
//! a component is `reached_by: network`, and refuses every flag only a served run reads with exit
//! 2 before running anything, as `run --help` says.
//!
//! Red cases are findings: `--observe-requests` and `--observe-series` are served-only flags that
//! require `--observe` and exit 2 without it, but beside `--stdin` clap excuses the missing
//! `--observe` (an argument that conflicts with a required one excuses it) and the run reads stdin;
//! `--log-format json` beside `--stdin` exits 2 only where a component is networked and only after
//! admission, where the nine flags clap refuses exit 2 on every specification and environment.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PAGE: &str = "website/docs/examples/rally.md";
const RALLY: &str = "examples/rally/ess";
const LINES: &str = "crates/spindle-cli/tests/fixtures/stdin-lines";
const LAPTOP: &str = "examples/environments/laptop.toml";
/// Refuses rally at admission (no `timer`), exit 3.
const BROWSER: &str = "examples/environments/browser.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs `b10x-spindle` with `arguments` and `input` on stdin, the environment variables of `env`
/// set. Every case here ends on its own: a run reading stdin ends at its end, and a refused one
/// before reading it.
fn run(arguments: &[&str], env: &[(&str, &str)], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .envs(env.iter().copied())
        .env_remove("SPINDLE_SETTING_CONSOLE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // A run refused before it reads stdin closes the pipe; the write may then fail.
    let _ = stdin.write_all(input.as_bytes());
    drop(stdin);
    child.wait_with_output().expect("b10x-spindle runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The page's input lines and its output block, byte for byte, from "Drive it by hand".
fn page() -> (String, String) {
    let page = std::fs::read_to_string(root().join(PAGE)).expect("the rally page reads");
    let start = page
        .find("## Drive it by hand\n")
        .expect("the page has the section");
    let section = &page[start..];
    let section = &section[..section[3..]
        .find("\n## ")
        .map_or(section.len(), |end| end + 3)];
    let fenced = |language: &str| -> String {
        let open = format!("```{language}\n");
        let at = section.find(&open).expect("the section has the block") + open.len();
        let body = &section[at..];
        body[..body.find("```\n").expect("the block closes")].to_owned()
    };
    let input = fenced("json");
    let console = fenced("console");
    let (command, output) = console
        .split_once('\n')
        .expect("the console block has a command line");
    assert!(command.contains(" --stdin "), "{command}");
    (input, output.to_owned())
}

/// The page's output block is what a run prints, byte for byte: key order, separators and the
/// line ends, not only the same JSON values.
#[test]
fn the_page_output_block_is_byte_equal_to_the_run() {
    let (input, expected) = page();
    let directory = tempfile::tempdir().expect("a state directory");
    let state = directory.path().join("rally.sqlite");
    let state = state.to_str().expect("a UTF-8 path");
    let output = run(
        &[
            "run",
            RALLY,
            "--environment",
            LAPTOP,
            "--stdin",
            "--state",
            state,
        ],
        &[],
        &input,
    );
    assert_eq!(text(&output.stdout), expected, "{output:?}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// `--log-format text` is the default: given explicitly beside `--stdin` it is not a served-only
/// flag, and the run reads the page's lines as without it.
#[test]
fn stdin_with_log_format_text_given_reads_the_lines() {
    let (input, expected) = page();
    let output = run(
        &[
            "run",
            RALLY,
            "--environment",
            LAPTOP,
            "--stdin",
            "--log-format",
            "text",
        ],
        &[],
        &input,
    );
    assert_eq!(text(&output.stdout), expected, "{output:?}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        text(&output.stderr).contains("b10x-spindle: line 3 was rejected"),
        "{output:?}"
    );
}

/// `--console` and the setting `SPINDLE_SETTING_CONSOLE` are documented as ignored where `run`
/// does not serve HTTP: beside `--stdin` neither refuses the run.
#[test]
fn stdin_ignores_the_console_flag_and_its_setting() {
    let (input, expected) = page();
    for (arguments, env) in [
        (&["--console", "on"][..], &[][..]),
        (&[][..], &[("SPINDLE_SETTING_CONSOLE", "on")][..]),
    ] {
        let mut all = vec!["run", RALLY, "--environment", LAPTOP, "--stdin"];
        all.extend_from_slice(arguments);
        let output = run(&all, env, &input);
        assert_eq!(text(&output.stdout), expected, "{arguments:?} {env:?}");
        assert_eq!(output.status.code(), Some(1), "{output:?}");
    }
}

/// `--stdin` and `--scenario` are two ways to drive a run: given together, exit 2, nothing run.
#[test]
fn stdin_with_scenario_exits_2() {
    let output = run(
        &[
            "run",
            RALLY,
            "--environment",
            LAPTOP,
            "--stdin",
            "--scenario",
            "absent.yaml",
        ],
        &[],
        "",
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(text(&output.stderr).contains("--scenario"), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

/// Where no component is networked, `--stdin` changes nothing: the run reads its lines.
#[test]
fn stdin_on_a_spec_without_a_networked_component_reads_the_lines() {
    let output = run(
        &["run", LINES, "--environment", LAPTOP, "--stdin"],
        &[],
        "{\"command\": \"shelf.books.AddBook\", \"input\": {\"title\": \"Emma\"}}\n",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let answer: serde_json::Value =
        serde_json::from_str(text(&output.stdout).trim()).expect("one JSON line");
    assert_eq!(answer["outcome"], "added", "{answer}");
}

/// RED. `--observe-requests` and `--observe-series` are read only by `--observe`, which only a
/// served run reads; without `--stdin` either exits 2 for want of `--observe`. Beside `--stdin`
/// clap excuses the missing `--observe`, since `--stdin` conflicts with it, and the run reads
/// stdin and exits 0, the flag ignored.
#[test]
fn stdin_refuses_the_observe_flags_that_require_observe() {
    for flag in [
        &["--observe-requests", "5"][..],
        &["--observe-series", "aggregated"][..],
    ] {
        for spec in [RALLY, LINES] {
            let mut all = vec!["run", spec, "--environment", LAPTOP, "--stdin"];
            all.extend_from_slice(flag);
            let output = run(&all, &[], "");
            assert_eq!(
                output.status.code(),
                Some(2),
                "{spec} {flag:?} is a flag only a served run reads: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{output:?}");
        }
    }
}

/// RED. `run --help` says `--stdin` given with `--log-format json` exits 2 before running anything;
/// on a specification no component of which is networked it exits 1, where `--stdin --port 0`
/// exits 2 on the same specification.
#[test]
fn stdin_with_log_format_json_exits_2_where_no_component_is_networked() {
    let port = run(
        &[
            "run",
            LINES,
            "--environment",
            LAPTOP,
            "--stdin",
            "--port",
            "0",
        ],
        &[],
        "",
    );
    assert_eq!(port.status.code(), Some(2), "{port:?}");
    let json = run(
        &[
            "run",
            LINES,
            "--environment",
            LAPTOP,
            "--stdin",
            "--log-format",
            "json",
        ],
        &[],
        "",
    );
    assert_eq!(json.status.code(), Some(2), "{json:?}");
}

/// RED. `--stdin --log-format json` is refused only after admission: on an environment that
/// refuses the specification it prints the admission and exits 3, where `--stdin --port 0` exits 2
/// and prints nothing on stdout.
#[test]
fn stdin_with_log_format_json_exits_2_before_admission() {
    let port = run(
        &[
            "run",
            RALLY,
            "--environment",
            BROWSER,
            "--stdin",
            "--port",
            "0",
        ],
        &[],
        "",
    );
    assert_eq!(port.status.code(), Some(2), "{port:?}");
    let json = run(
        &[
            "run",
            RALLY,
            "--environment",
            BROWSER,
            "--stdin",
            "--log-format",
            "json",
        ],
        &[],
        "",
    );
    assert_eq!(json.status.code(), Some(2), "{json:?}");
    assert!(json.stdout.is_empty(), "nothing ran: {json:?}");
}
