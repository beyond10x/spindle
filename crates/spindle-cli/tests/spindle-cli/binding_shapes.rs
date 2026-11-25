//! `run --http` serving two `ess-http/1` shapes ESS 0.99.0 admits (beyond10x/ess#755) over its
//! listener (`story:ess-099-path-types-and-wrapped-views`): a path variable whose source is an
//! unconstrained newtype of `Uuid`, on a command route and on a view route, and a view answered
//! under one member, `{<member>: array}` and `{<member>: single}`. A newtype with a prefix and an
//! alphabet in the path stays refused by ESS (`ESS-HTTP-009`), and `run` prints that refusal and
//! serves nothing.
//!
//! The fixture `crates/spindle-ingress/tests/fixtures/locker` declares one component, `locker`,
//! reached over the network: `RentLocker` creates a locker, `TouchLocker` sets its values,
//! `AllLockers` lists the lockers and `LockerById` the one an id names. The binding is written
//! into a scratch directory with the specification's digest.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const FIXTURE: &str = "crates/spindle-ingress/tests/fixtures/locker";
const LAPTOP: &str = "examples/environments/laptop.toml";

const FIRST: &str = "6f9619ff-8b86-4d01-b42d-00c04fc964ff";
const SECOND: &str = "0e2a7d4e-3c1b-4f5a-9d8e-7a6b5c4d3e2f";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

/// The routes the served binding declares: the consumer's shape, a newtype of `Uuid` as `{id}`
/// on a command and a view, and the lockers listed under one member.
const ROUTES: &str = "  - command: locker.cabinet.RentLocker
    method: post
    path: /lockers
    outcomes:
      rented: {status: 201}
  - command: locker.cabinet.TouchLocker
    method: put
    path: /lockers/{id}/touch
    parameters: {id: input.locker_id}
    outcomes:
      touched: {status: 200}
  - view: locker.cabinet.LockerById
    method: get
    path: /lockers/{id}
    parameters: {id: param.id}
    body: {locker: single}
    empty: {status: 404}
  - view: locker.cabinet.AllLockers
    method: get
    path: /lockers
    body: {lockers: array}
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The binding `routes` declares for the fixture, written into `directory`.
fn binding(directory: &Path, routes: &str) -> PathBuf {
    let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
    let path = directory.join("locker-http.yaml");
    std::fs::write(
        &path,
        format!(
            "type: ess-http/1\nspecification:\n  system: locker\n  version: v1\n  source_digest: \
             sha256:{}\nroutes:\n{routes}",
            loaded.ir.source_digest()
        ),
    )
    .expect("the binding is written");
    path
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    /// One HTTP/1.1 exchange on its own connection: the status and the body.
    fn exchange(&self, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
        let mut stream =
            TcpStream::connect_timeout(&self.address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        let body = body.map(Value::to_string).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            self.address,
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
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("a status line: {head:?}"));
        (status, body.to_owned())
    }
}

/// `b10x-spindle run <fixture> --environment laptop --http <binding>`, its state in `directory`,
/// once it prints where it listens.
fn serve(directory: &Path, binding: &Path) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", directory)
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(directory.join("state"))
        .arg("--http")
        .arg(binding)
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
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON ({error})")
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served { child, address }
}

fn rented(id: &str) -> Value {
    json!({
        "locker_id": id,
        "code": "front door",
        "open": true,
        "rent": 12.5,
        "since": "2026-11-25T10:00:00Z",
        "term": "PT30M",
        "serial": "1b4e28ba-2fa1-41d2-883f-0016d3cca427",
        "badge": "b-cd",
    })
}

#[test]
fn run_serves_a_uuid_id_in_the_path_and_views_under_one_member() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let served = serve(scratch.path(), &binding(scratch.path(), ROUTES));
    for id in [FIRST, SECOND] {
        let (status, body) = served.exchange("POST", "/lockers", Some(&rented(id)));
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = served.exchange("GET", &format!("/lockers/{FIRST}"), None);
    assert_eq!(status, 200, "{body}");
    let one: Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(one["locker"]["locker_id"], FIRST, "{body}");
    assert_eq!(one.as_object().map(|one| one.len()), Some(1), "{body}");

    let mut touch = rented(FIRST);
    touch
        .as_object_mut()
        .expect("an object")
        .remove("locker_id");
    touch["code"] = json!("back door");
    let (status, body) = served.exchange("PUT", &format!("/lockers/{FIRST}/touch"), Some(&touch));
    assert_eq!(status, 200, "{body}");

    let (status, body) = served.exchange("GET", "/lockers", None);
    assert_eq!(status, 200, "{body}");
    let all: Value = serde_json::from_str(&body).expect("JSON");
    let codes: Vec<(&Value, &Value)> = all["lockers"]
        .as_array()
        .unwrap_or_else(|| panic!("no lockers member: {body}"))
        .iter()
        .map(|row| (&row["locker_id"], &row["code"]))
        .collect();
    assert!(
        codes.contains(&(&json!(FIRST), &json!("back door")))
            && codes.contains(&(&json!(SECOND), &json!("front door"))),
        "{body}"
    );

    let (status, body) =
        served.exchange("GET", "/lockers/00000000-0000-4000-8000-000000000000", None);
    assert_eq!((status, body.as_str()), (404, ""), "the empty single");
    for (method, path, input) in [
        ("GET", "/lockers/not-a-uuid", None),
        ("PUT", "/lockers/not-a-uuid/touch", Some(&touch)),
    ] {
        let (status, body) = served.exchange(method, path, input);
        assert_eq!(status, 400, "{method} {path}: {body}");
    }
}

#[test]
fn run_refuses_a_constrained_newtype_in_the_path_as_ess_does() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let refused = binding(
        scratch.path(),
        "  - command: locker.cabinet.TouchLocker
    method: put
    path: /badges/{badge}
    parameters: {badge: input.badge}
    outcomes:
      touched: {status: 200}
",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", scratch.path())
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(scratch.path().join("state"))
        .arg("--http")
        .arg(&refused)
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty(), "nothing is served: {output:?}");
    for named in [
        "ESS-HTTP-009",
        "`input.badge` has type `locker.cabinet.Badge`, which a path segment cannot carry",
    ] {
        assert!(stderr.contains(named), "{named} not in: {stderr}");
    }
}
