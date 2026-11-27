//! `run --console <on|off>` (`SPINDLE_SETTING_CONSOLE`): with the console off, every console
//! route of [`spindle_ingress::Console::ALL`] answers `404` as an unrouted path does, whatever
//! secret a request carries, and the routes ESS publishes are served as before. Without either,
//! the console is on where the run listens on loopback and off where `--address` is off it.
//!
//! The fixture `tests/fixtures/trusted-caller` is served with `--trusted-caller-header`: the
//! actor `Member` may record a visit, `Auditor` alone may read `VisitLog`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_cli::cli::{Switch, console_served};
use spindle_ingress::Console;

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "front";
const HEADER: &str = "X-Spindle-Caller";
const SECRET_HEADER: &str = "Spindle-Proxy-Secret";
const SECRET: &str = "c0nsole-proxy-token-3e9b";
const SETTING: &str = "SPINDLE_SETTING_CONSOLE";
const MEMBER: &str = "porch.visit.Member";
const AUDITOR: &str = "porch.visit.Auditor";
const RECORD: &str = "porch.visit.RecordVisit";
const LOG: &str = "porch.visit.VisitLog";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The method and path the served surface puts `operation` at.
fn route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the trusted-caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    /// The directory of the secret file the run reads, kept until the run stops.
    _secret: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on the fixture and laptop trusting [`HEADER`] with the proxy's secret [`SECRET`],
/// `arguments` added and `SPINDLE_SETTING_CONSOLE` set to `setting`, or unset.
fn serve(arguments: &[&str], setting: Option<&str>) -> Served {
    let secret = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = secret.path().join("proxy-secret");
    std::fs::write(&path, format!("{SECRET}\n")).expect("the secret file is written");
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(["--trusted-caller-header", HEADER, "--trusted-caller-secret"])
        .arg(&path)
        .args(arguments)
        .env_remove(SETTING)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
        panic!("run stopped before listening ({status}): {stderr}");
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
        _secret: secret,
    }
}

/// One HTTP/1.1 exchange on its own connection carrying the proxy's secret and `headers`: the
/// status and the body, as JSON where it is JSON.
fn exchange(
    served: &Served,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    let extra: String = headers
        .iter()
        .chain(&[(SECRET_HEADER, SECRET)])
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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

/// One request for `operation` as `actor` proves it.
fn call(served: &Served, operation: &str, actor: &Value, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(operation);
    let proof = actor.to_string();
    exchange(served, &method, &path, &[(HEADER, &proof)], body)
}

/// Records [`VISIT`] for [`PERSON`].
fn record(served: &Served) {
    let member = json!({"actor": MEMBER, "caller": {"person_id": PERSON}});
    let (status, recorded) = call(served, RECORD, &member, Some(&json!({"visit_id": VISIT})));
    assert_eq!(status, 202, "{recorded}");
    assert_eq!(recorded["outcome"], "recorded", "{recorded}");
}

/// The rows of `VisitLog`, read as the auditor.
fn log(served: &Served) -> Value {
    let (status, rows) = call(served, LOG, &json!({"actor": AUDITOR}), None);
    assert_eq!(status, 200, "{rows}");
    rows["rows"].clone()
}

/// Each console route of [`Console::ALL`] with the proxy's secret: its route and its answer.
fn console_answers(served: &Served) -> Vec<(Console, u16, Value)> {
    Console::ALL
        .into_iter()
        .map(|console| {
            let world = json!({"rows": []});
            // A replay names a dead letter no run keeps (story:dead-letters-kept-and-replayed).
            let letter = json!({"component": "none", "id": 1});
            let body = match console {
                Console::Reset => Some(&world),
                Console::Replay => Some(&letter),
                _ => None,
            };
            let method = console.method().as_str().to_uppercase();
            let path = match console {
                Console::History => format!(
                    "{}?entity=porch.visit.Visit&identity={VISIT}",
                    console.path()
                ),
                _ => console.path().to_owned(),
            };
            let (status, answer) = exchange(served, &method, &path, &[], body);
            (console, status, answer)
        })
        .collect()
}

/// Whether `console` answered as a route the run serves: `200`, or, for the replay of a dead
/// letter no run keeps, `404` naming the letter, not the path (story:dead-letters-kept-and-replayed).
fn served_answer(console: Console, status: u16, answer: &Value) -> bool {
    match console {
        Console::Replay => {
            status == 404
                && answer["refused"]
                    .as_str()
                    .is_some_and(|why| why.starts_with("no dead letter 1 of none"))
        }
        _ => status == 200,
    }
}

/// The answer an unrouted `path` gets.
fn unrouted(path: &str) -> Value {
    json!({"refused": format!("no route serves {path}")})
}

/// Every console route answers `404`, as the unrouted path `GET /api/unrouted` does.
fn assert_console_off(served: &Served) {
    let (status, answer) = exchange(served, "GET", "/api/unrouted", &[], None);
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer, unrouted("/api/unrouted"));
    for (console, status, answer) in console_answers(served) {
        assert_eq!(status, 404, "{console}: {answer}");
        assert_eq!(
            answer,
            unrouted(console.path()),
            "{console} answers as an unrouted path"
        );
    }
}

#[test]
fn console_routes_off_answer_404_with_the_secret() {
    let served = serve(&["--console", "off"], None);
    record(&served);
    assert_console_off(&served);
    let rows = log(&served);
    assert_eq!(
        rows.as_array().map(Vec::len),
        Some(1),
        "the row written before the refused reset is still there: {rows}"
    );
    assert_eq!(rows[0]["visit_id"], VISIT, "{rows}");
}

#[test]
fn console_routes_off_leaves_the_published_routes_served() {
    let served = serve(&["--console", "off"], None);
    assert_eq!(log(&served), json!([]), "the view answers before a command");
    record(&served);
    assert_eq!(
        log(&served),
        json!([{"visit_id": VISIT, "person_id": PERSON}]),
        "the command wrote its row and the view reads it"
    );
}

#[test]
fn console_routes_on_by_default_on_loopback() {
    let served = serve(&[], None);
    record(&served);
    for (console, status, answer) in console_answers(&served) {
        assert!(
            served_answer(console, status, &answer),
            "{console}: {status} {answer}"
        );
    }
    assert_eq!(log(&served), json!([]), "the reset ran");
    let served = serve(&["--console", "on"], None);
    for (console, status, answer) in console_answers(&served) {
        assert!(
            served_answer(console, status, &answer),
            "--console on, {console}: {status} {answer}"
        );
    }
}

#[test]
fn console_off_by_setting() {
    let served = serve(&[], Some("off"));
    record(&served);
    assert_console_off(&served);
    assert_eq!(log(&served).as_array().map(Vec::len), Some(1));
}

/// The function `run` decides the console with: off where the address is off loopback and the
/// switch is not given, as given otherwise, on where it is not given on loopback. No run binds an
/// address off loopback here.
#[test]
fn console_off_by_default_off_loopback() {
    let loopback: [IpAddr; 3] = [
        Ipv4Addr::LOCALHOST.into(),
        Ipv4Addr::new(127, 4, 5, 6).into(),
        Ipv6Addr::LOCALHOST.into(),
    ];
    let exposed: [IpAddr; 4] = [
        Ipv4Addr::UNSPECIFIED.into(),
        Ipv4Addr::new(10, 0, 0, 7).into(),
        Ipv6Addr::UNSPECIFIED.into(),
        "::ffff:127.0.0.1".parse().expect("an IPv6 address"),
    ];
    for address in exposed {
        assert!(!console_served(None, address), "{address}, no switch");
        assert!(console_served(Some(Switch::On), address), "{address}, on");
        assert!(
            !console_served(Some(Switch::Off), address),
            "{address}, off"
        );
    }
    for address in loopback {
        assert!(console_served(None, address), "{address}, no switch");
        assert!(console_served(Some(Switch::On), address), "{address}, on");
        assert!(
            !console_served(Some(Switch::Off), address),
            "{address}, off"
        );
    }
}
