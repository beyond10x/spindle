//! Adversary cases for `story:caller-attributes-on-loopback` (wave 27, unit U3): the one stderr
//! line `run` prints naming `--fixed-actor` with `--fixed-caller` and `--trusted-caller-header`
//! with `--trusted-caller-secret` is printed exactly once, and only where a served command reads
//! its caller and the run gives neither setting.
//!
//! The fixture `tests/fixtures/adversary-w27-u3-callers` declares one component, `front`, reached
//! over the network, whose two commands `RecordVisit` and `NoteVisit` each read the caller's
//! `person_id`, and whose `WaveVisit` reads none, so the run serves something. `tests/fixtures/ingress`
//! declares a component whose commands read no caller.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const CALLERS: &str = "crates/spindle-cli/tests/fixtures/adversary-w27-u3-callers";
const PLAIN: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";
const MEMBER: &str = "stoop.visit.Member";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const FLAGS: [&str; 4] = [
    "--fixed-actor",
    "--fixed-caller",
    "--trusted-caller-header",
    "--trusted-caller-secret",
];
/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(50);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What a loopback `run` of `spec` with `arguments` printed on stderr once it listened (or
/// exited), and whether it listened.
fn started(spec: &str, arguments: &[&str]) -> (bool, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (printed, stderr_read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = printed.send(text);
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
    let listened = line.contains("listening");
    let _ = child.kill();
    let _ = child.wait();
    let stderr = stderr_read
        .recv_timeout(STARTUP)
        .expect("stderr is read to its end once the run stops");
    (listened, stderr)
}

/// The stderr lines naming all four settings.
fn naming(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|line| FLAGS.iter().all(|flag| line.contains(flag)))
        .collect()
}

/// Two routes whose commands read the caller, neither setting given: both are named not served,
/// and the line naming the settings is printed once, not once per route.
#[test]
fn the_settings_line_is_printed_once_for_two_routes_reading_the_caller() {
    let (listened, stderr) = started(CALLERS, &[]);
    assert!(listened, "run listens on loopback: {stderr}");
    let unserved = stderr
        .lines()
        .filter(|line| {
            line.starts_with("b10x-spindle: not served: ")
                && line.contains("which no HTTP caller proves here")
        })
        .count();
    assert_eq!(unserved, 2, "both routes are named not served: {stderr}");
    assert_eq!(naming(&stderr).len(), 1, "{stderr}");
}

/// A run that fixes its caller serves both routes and prints no line naming the settings.
#[test]
fn the_settings_line_is_not_printed_where_the_run_fixes_its_caller() {
    let caller = format!(r#"{{"person_id": "{PERSON}"}}"#);
    let (listened, stderr) = started(
        CALLERS,
        &["--fixed-actor", MEMBER, "--fixed-caller", &caller],
    );
    assert!(listened, "run listens on loopback: {stderr}");
    assert!(naming(&stderr).is_empty(), "{stderr}");
    assert!(
        !stderr.contains("which no HTTP caller proves here"),
        "both routes are served: {stderr}"
    );
}

/// A run where each request proves its caller prints no line naming the settings.
#[test]
fn the_settings_line_is_not_printed_where_each_request_proves_its_caller() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let secret = directory.path().join("proxy-secret");
    std::fs::write(&secret, "a-proxy-secret-of-some-length").expect("the secret is written");
    let secret = secret.to_str().expect("a UTF-8 path").to_owned();
    let (listened, stderr) = started(
        CALLERS,
        &[
            "--trusted-caller-header",
            "X-Stoop-Caller",
            "--trusted-caller-secret",
            &secret,
        ],
    );
    assert!(listened, "run listens on loopback: {stderr}");
    assert!(naming(&stderr).is_empty(), "{stderr}");
}

/// A run whose commands read no caller prints no line naming the settings.
#[test]
fn the_settings_line_is_not_printed_where_no_command_reads_the_caller() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let state = directory.path().to_str().expect("a UTF-8 path").to_owned();
    let (listened, stderr) = started(PLAIN, &["--state", &state]);
    assert!(listened, "run listens on loopback: {stderr}");
    assert!(naming(&stderr).is_empty(), "{stderr}");
    assert!(
        !stderr.contains("caller attributes"),
        "no line speaks of caller attributes: {stderr}"
    );
}

/// A run off loopback without the proxy's settings exits before it listens, and prints no line
/// naming the settings beside the refusal: the refusal names the two it needs.
#[test]
fn the_settings_line_is_not_printed_off_loopback() {
    let (listened, stderr) = started(CALLERS, &["--address", "0.0.0.0"]);
    assert!(!listened, "run does not listen off loopback: {stderr}");
    assert!(
        stderr.contains("is not loopback"),
        "the run is refused before listening: {stderr}"
    );
    assert!(naming(&stderr).is_empty(), "{stderr}");
}
