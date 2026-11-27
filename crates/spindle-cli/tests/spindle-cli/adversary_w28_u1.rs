//! Adversary pass on `story:console-routes-off` (wave 28 U1): `run --console <on|off>`
//! (`SPINDLE_SETTING_CONSOLE`).
//!
//! Attacked here: the off-loopback default end to end (the unit's own cases test it only as a
//! call of `console_served`, so a run that decided with a fixed loopback address would pass
//! them); the flag's text "Off, each answers `404` as an unrouted path does" on a binding whose
//! route carries a console path; and the setting read from the environment on a `run
//! --scenario`, which the flag conflicts with.
//!
//! The address off loopback a run binds here is `::ffff:127.0.0.1`: `spindle_ingress::exposed`
//! counts it off loopback, and it accepts only connections from this host.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_ingress::Console;

const TRUSTED: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const RESERVED: &str = "crates/spindle-cli/tests/fixtures/reserved";
const CONSOLE_PATH_HTTP: &str =
    "crates/spindle-cli/tests/fixtures/adversary-w28-u1-console-path-http.yaml";
const REPLAY: &str = "crates/spindle-cli/tests/fixtures/replay";
const HOLDS: &str = "crates/spindle-cli/tests/fixtures/replay-scenarios/holds.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const HEADER: &str = "X-Spindle-Caller";
const SECRET_HEADER: &str = "Spindle-Proxy-Secret";
const SECRET: &str = "adv-w28-u1-proxy-token-71c4";
const SETTING: &str = "SPINDLE_SETTING_CONSOLE";
const MAPPED_LOOPBACK: &str = "::ffff:127.0.0.1";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Served {
    child: Child,
    address: SocketAddr,
    _dir: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run spec` on laptop trusting [`HEADER`] with the proxy's secret, `arguments` added and
/// `SPINDLE_SETTING_CONSOLE` set to `setting`, or unset.
fn serve(spec: &str, arguments: &[&str], setting: Option<&str>) -> Served {
    serve_as(spec, true, arguments, setting)
}

/// As [`serve`], trusting the proxy only where `trusted`.
fn serve_as(spec: &str, trusted: bool, arguments: &[&str], setting: Option<&str>) -> Served {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let secret = dir.path().join("proxy-secret");
    std::fs::write(&secret, format!("{SECRET}\n")).expect("the secret file is written");
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(dir.path().join("state"))
        .env_remove(SETTING)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if trusted {
        command
            .args(["--trusted-caller-header", HEADER, "--trusted-caller-secret"])
            .arg(&secret);
    }
    command.args(arguments);
    if let Some(setting) = setting {
        command.env(SETTING, setting);
    }
    let mut child = command.spawn().expect("b10x-spindle starts");
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
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let stderr = stderr_read.recv_timeout(ANSWER).unwrap_or_default();
        panic!("run {arguments:?} stopped before listening ({status}): {stderr}");
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _dir: dir,
    }
}

/// One exchange carrying the proxy's secret: the status and the body, as JSON where it is JSON.
fn exchange(served: &Served, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n{SECRET_HEADER}: {SECRET}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        served.address,
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_owned()));
    (status, body)
}

/// Each console route of [`Console::ALL`] with the proxy's secret, a reset carrying an empty
/// world: the route, its status and its answer.
fn console_answers(served: &Served) -> Vec<(Console, u16, Value)> {
    Console::ALL
        .into_iter()
        .map(|console| {
            let world = json!({"rows": []});
            let body = (console == Console::Reset).then_some(&world);
            let method = console.method().as_str().to_uppercase();
            let path = match console {
                Console::History => format!(
                    "{}?entity=porch.visit.Visit&identity=5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f",
                    console.path()
                ),
                _ => console.path().to_owned(),
            };
            let (status, answer) = exchange(served, &method, &path, body);
            (console, status, answer)
        })
        .collect()
}

/// The status a served console route answers here: `409` for both methods of `/api/clock`, which
/// a run without `--clock` serves and refuses to step (`story:served-clock-steps`), `200` for
/// every other.
fn served_status(console: Console) -> u16 {
    match console {
        Console::Clock | Console::Step => 409,
        _ => 200,
    }
}

fn unrouted(path: &str) -> Value {
    json!({"refused": format!("no route serves {path}")})
}

/// End to end, a run on an address off loopback serves no console route without the flag or
/// the setting, serves them with `--console on` or `SPINDLE_SETTING_CONSOLE=on`, and the flag
/// wins over the setting. The unit's `console_off_by_default_off_loopback` calls
/// `console_served` alone, and every run of its module listens on loopback.
#[test]
fn off_loopback_the_run_itself_serves_no_console_route_by_default() {
    let address = ["--address", MAPPED_LOOPBACK];
    let exposed = serve(TRUSTED, &address, None);
    assert!(
        exposed.address.ip().is_ipv6() && !exposed.address.ip().is_loopback(),
        "the run listens off loopback as `exposed` counts it: {}",
        exposed.address
    );
    for (console, status, answer) in console_answers(&exposed) {
        assert_eq!(status, 404, "default off loopback, {console}: {answer}");
        assert_eq!(answer, unrouted(console.path()), "{console}");
    }
    drop(exposed);

    let flagged = serve(TRUSTED, &[address[0], address[1], "--console", "on"], None);
    for (console, status, answer) in console_answers(&flagged) {
        assert_eq!(
            status,
            served_status(console),
            "--console on off loopback, {console}: {answer}"
        );
    }
    drop(flagged);

    let set = serve(TRUSTED, &address, Some("on"));
    for (console, status, answer) in console_answers(&set) {
        assert_eq!(
            status,
            served_status(console),
            "setting on off loopback, {console}: {answer}"
        );
    }
    drop(set);

    let overridden = serve(
        TRUSTED,
        &[address[0], address[1], "--console", "off"],
        Some("on"),
    );
    for (console, status, answer) in console_answers(&overridden) {
        assert_eq!(
            status, 404,
            "the flag wins over the setting, {console}: {answer}"
        );
    }
}

/// A binding route whose template carries a console path (`/{a}/{b}` carries `/api/model`)
/// exits 1 before listening with the console on (`Surface::with_console`, `Shadowed`), and with
/// it off too, on loopback and off it: the switch never changes which route a console path
/// reaches (coordinator's decision, wave 28 U1 correction round 1; this case first asserted that
/// with the console off the run starts and each console path answers `404`).
#[test]
fn with_the_console_off_a_binding_route_on_a_console_path_is_refused_at_start() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let secret = dir.path().join("proxy-secret");
    std::fs::write(&secret, format!("{SECRET}\n")).expect("the secret file is written");
    let secret = secret.to_str().expect("a UTF-8 path");
    let trusted = [
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        secret,
    ];
    let cases: [(&str, Vec<&str>); 3] = [
        ("console on, loopback", vec!["--console", "on"]),
        ("console off, loopback", vec!["--console", "off"]),
        (
            "console off, off loopback",
            [
                &["--console", "off", "--address", MAPPED_LOOPBACK][..],
                &trusted[..],
            ]
            .concat(),
        ),
    ];
    for (case, arguments) in cases {
        let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .args(["run", RESERVED, "--environment", LAPTOP, "--state"])
            .arg(dir.path().join(case.replace([' ', ','], "-")))
            .args(["--http", CONSOLE_PATH_HTTP])
            .args(&arguments)
            .env_remove(SETTING)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let started = std::time::Instant::now();
        while child.try_wait().expect("the child is waited on").is_none() {
            if started.elapsed() > STARTUP {
                let _ = child.kill();
                let output = child.wait_with_output();
                panic!("{case}: run did not exit within {STARTUP:?}: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child.wait_with_output().expect("the output is read");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{case}: {stderr}");
        assert!(
            !stdout.contains("listening"),
            "{case}: nothing listened: {stdout}"
        );
        assert!(
            stderr.contains("the console route GET /api/model would shadow view `notes.shelf.NotesOn`, which is served at GET /{a}/{b}"),
            "{case}: the refusal names the console route and the route it shadows: {stderr}"
        );
    }
}

/// `--console` conflicts with `--scenario`; the setting is read from the environment for that
/// flag. A process whose environment carries `SPINDLE_SETTING_CONSOLE` (an image or shell that
/// sets it, as the flag's text invites) runs `run --scenario` exactly as one without it.
#[test]
fn the_setting_in_the_environment_does_not_refuse_a_scenario_run() {
    let replay = |setting: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .current_dir(root())
            .args(["run", REPLAY, "--environment", LAPTOP, "--scenario", HOLDS])
            .env_remove(SETTING)
            .stdin(Stdio::null());
        if let Some(setting) = setting {
            command.env(SETTING, setting);
        }
        command.output().expect("b10x-spindle runs")
    };
    let without = replay(None);
    assert_eq!(without.status.code(), Some(0), "{without:?}");
    for setting in ["off", "on"] {
        let with = replay(Some(setting));
        assert_eq!(
            with.status.code(),
            Some(0),
            "{SETTING}={setting} on `run --scenario`: {}",
            String::from_utf8_lossy(&with.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&with.stdout),
            String::from_utf8_lossy(&without.stdout),
            "{SETTING}={setting}"
        );
    }
}
