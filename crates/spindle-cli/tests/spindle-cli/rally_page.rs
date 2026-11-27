//! The rally page's "Drive it by hand" runs as written (story:rally-example-reads-stdin): the
//! table of `examples/rally/ess` is `reached_by: network`, so `run` serves HTTP unless `--stdin`
//! is given, and with it reads the page's JSON lines, fires the event bindings and prints what the
//! page prints. The command, the input and the output are read from
//! `website/docs/examples/rally.md`, so the page and the run cannot drift apart.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

const PAGE: &str = "website/docs/examples/rally.md";
const RALLY: &str = "examples/rally/ess";
const LAPTOP: &str = "examples/environments/laptop.toml";

/// How long a run reading stdin may take to answer the page's three lines, or a served run to
/// print where it listens; a served run never ends on its own, so a run still going then is
/// killed and fails the case.
const BOUND: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The body of each fenced block of `section` in the page, with its language, in page order.
fn blocks(page: &str, section: &str) -> Vec<(String, String)> {
    let heading = format!("## {section}\n");
    let start = page
        .find(&heading)
        .unwrap_or_else(|| panic!("{PAGE} has no section {section:?}"));
    let rest = &page[start + heading.len()..];
    let body = rest.find("\n## ").map_or(rest, |end| &rest[..end]);
    let mut found = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        if let Some(language) = line.strip_prefix("```") {
            let text: Vec<&str> = lines.by_ref().take_while(|line| *line != "```").collect();
            found.push((language.to_owned(), text.join("\n")));
        }
    }
    found
}

/// A child's exit status, stdout and stderr, or `None` where it ran past `bound` and was killed.
fn bounded(mut child: Child, bound: Duration) -> Option<(ExitStatus, String, String)> {
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("the child can be waited on") {
            break Some(status);
        }
        if started.elapsed() > bound {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out.join().expect("stdout is read");
    let stderr = err.join().expect("stderr is read");
    status.map(|status| (status, stdout, stderr))
}

fn json_lines(text: &str) -> Vec<Value> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{line:?} is not one JSON value: {error}"))
        })
        .collect()
}

#[test]
fn the_rally_example_reads_stdin_as_its_page_says() {
    let page = std::fs::read_to_string(root().join(PAGE)).expect("the rally page reads");
    assert!(
        !page.contains("bindings do not fire"),
        "the page still says bindings do not fire on stdin"
    );
    let blocks = blocks(&page, "Drive it by hand");
    let input = blocks
        .iter()
        .find(|(language, _)| language == "json")
        .map(|(_, text)| text.clone())
        .expect("the section shows requests.jsonl");
    let console = blocks
        .iter()
        .find(|(language, _)| language == "console")
        .map(|(_, text)| text.clone())
        .expect("the section shows the run");
    let mut console_lines = console.lines();
    let command = console_lines
        .next()
        .and_then(|line| line.strip_prefix("$ b10x-spindle "))
        .expect("the console block starts with the command");
    let (arguments, redirect) = command
        .split_once(" < ")
        .expect("the command reads requests.jsonl on stdin");
    assert_eq!(redirect, "requests.jsonl", "{command}");
    let expected = json_lines(&console_lines.collect::<Vec<_>>().join("\n"));

    let directory = tempfile::tempdir().expect("a state directory");
    let state = directory.path().join("rally.sqlite");
    let mut words: Vec<String> = arguments.split_whitespace().map(str::to_owned).collect();
    assert!(
        words.iter().any(|word| word == "--stdin"),
        "the page's command reads stdin: {command}"
    );
    let at = words
        .iter()
        .position(|word| word == "--state")
        .expect("the page's command names --state");
    // The page's state file, kept out of the tree.
    words[at + 1] = state.display().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(&words)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let _ = stdin.write_all(format!("{input}\n").as_bytes());
    drop(stdin);
    let (status, stdout, stderr) =
        bounded(child, BOUND).unwrap_or_else(|| panic!("`{command}` did not end within {BOUND:?}"));

    assert_eq!(
        json_lines(&stdout),
        expected,
        "the run prints the page's output block\nstderr: {stderr}"
    );
    assert_eq!(
        status.code(),
        Some(1),
        "the third line fails the run: {stderr}"
    );
    assert!(
        stderr.contains("line 3 was rejected"),
        "stderr names line 3: {stderr}"
    );
    assert!(
        !stderr.contains("line 1 was rejected") && !stderr.contains("line 2 was rejected"),
        "only line 3 is rejected: {stderr}"
    );
}

/// Every flag only a served run reads, with a value it would take; `--port` is the story's own.
const SERVED_ONLY: [&[&str]; 10] = [
    &["--port", "0"],
    &["--http", "binding.yaml"],
    &["--address", "127.0.0.1"],
    &["--fixed-actor", "rally.Player"],
    &["--trusted-caller-header", "X-Caller"],
    &["--trusted-caller-secret", "secret"],
    &["--observe"],
    &["--seed", "seed.json"],
    &["--drain", "1"],
    &["--log-format", "json"],
];

#[test]
fn stdin_on_a_networked_spec_refuses_served_only_flags() {
    for flag in SERVED_ONLY {
        let child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .args(["run", RALLY, "--environment", LAPTOP, "--stdin"])
            .args(flag)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let (status, stdout, stderr) = bounded(child, BOUND)
            .unwrap_or_else(|| panic!("run {flag:?} did not end within {BOUND:?}"));
        assert_eq!(
            status.code(),
            Some(2),
            "{flag:?}\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains(flag[0]),
            "stderr names {}: {stderr}",
            flag[0]
        );
        assert!(stdout.is_empty(), "{flag:?}: nothing ran: {stdout}");
    }
}

#[test]
fn a_networked_spec_without_stdin_still_serves() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", RALLY, "--environment", LAPTOP])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(BOUND);
    let _ = child.kill();
    let _ = child.wait();
    let line = line.unwrap_or_else(|error| panic!("run printed no line within {BOUND:?}: {error}"));
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    assert!(
        ready["listening"].is_string(),
        "the first line names where it listens: {ready}"
    );
    assert_eq!(
        ready["components"],
        serde_json::json!(["rally-table"]),
        "{ready}"
    );
}
