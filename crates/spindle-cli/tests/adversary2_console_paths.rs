//! Adversary pass 2 on `story:http-caller-proves-actor` (wave 19 U2): on a run with
//! `--trusted-caller-header` and `--observe`, every route the run serves itself (`/api/*`,
//! `/__spindle/*`) needs the proxy's secret. Attacked here with request-target spellings, methods,
//! versions and `Host` values that might reach console or reserved behaviour without it.
//!
//! The fixture is the unit's own, `tests/fixtures/trusted-caller`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "front";
const HEADER: &str = "X-Spindle-Caller";
const SECRET_HEADER: &str = "Spindle-Proxy-Secret";
const SECRET: &str = "adv2-proxy-token-5e1b";
const MEMBER: &str = "porch.visit.Member";
const AUDITOR: &str = "porch.visit.Auditor";
const RECORD: &str = "porch.visit.RecordVisit";
const LOG: &str = "porch.visit.VisitLog";
const VISIT_ENTITY: &str = "porch.visit.Visit";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const FORGED: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
const PLANTED: &str = "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

struct Served {
    child: Child,
    address: SocketAddr,
    _secret: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn trusting() -> Served {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("proxy-secret");
    std::fs::write(&path, format!("{SECRET}\n")).expect("the secret file is written");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--observe"])
        .args(["--trusted-caller-header", HEADER, "--trusted-caller-secret"])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut line);
        let _ = sender.send(line);
    });
    let line = receiver
        .recv_timeout(STARTUP)
        .unwrap_or_else(|_| panic!("run printed no line within {STARTUP:?}"));
    let ready: Value = serde_json::from_str(&line).expect("run's first line is JSON");
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _secret: directory,
    }
}

/// One raw request written verbatim on its own connection, read to its end: the status (0 where
/// the connection closed without one) and everything after the status line.
fn raw(address: SocketAddr, request: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    stream.write_all(request).expect("the request is written");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0);
    (status, answer)
}

fn request(
    method: &str,
    target: &str,
    version: &str,
    host: Option<&str>,
    head: &[(&str, &str)],
    body: &str,
) -> Vec<u8> {
    let mut text = format!("{method} {target} {version}\r\n");
    if let Some(host) = host {
        text.push_str(&format!("Host: {host}\r\n"));
    }
    text.push_str("Connection: close\r\n");
    for (name, value) in head {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str(&format!(
        "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    text.into_bytes()
}

fn json_body(answer: &str) -> Value {
    let body = answer.split("\r\n\r\n").nth(1).unwrap_or_default();
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn with_secret(served: &Served, method: &str, target: &str, head: &[(&str, &str)]) -> Value {
    let host = served.address.to_string();
    let mut head = head.to_vec();
    head.push((SECRET_HEADER, SECRET));
    let (status, answer) = raw(
        served.address,
        &request(method, target, "HTTP/1.1", Some(&host), &head, ""),
    );
    assert_eq!(status, 200, "{method} {target} with the secret: {answer}");
    json_body(&answer)
}

/// Request-target spellings of each run-owned route.
fn targets(address: SocketAddr) -> Vec<String> {
    let mut targets = Vec::new();
    for path in [
        "/api/events",
        "/api/model",
        "/api/reset",
        "/__spindle/requests",
        "/__spindle/metrics",
        "/__spindle/topology",
        "/__spindle/ui",
    ] {
        let (head, tail) = path[1..].split_once('/').expect("two segments");
        let encoded_head = head
            .bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect::<String>();
        let encoded_tail = tail
            .bytes()
            .map(|byte| format!("%{byte:02x}"))
            .collect::<String>();
        for spelled in [
            path.to_owned(),
            format!("{path}/"),
            format!("{path}//"),
            format!("/{path}"),
            format!("/{head}//{tail}"),
            format!("/{encoded_head}/{tail}"),
            format!("/{head}/{encoded_tail}"),
            format!("/{head}%2F{tail}"),
            format!("/{}/{tail}", head.to_uppercase()),
            format!("/{head}/{}", tail.to_uppercase()),
            format!("/{head}/../{head}/{tail}"),
            format!("/./{head}/{tail}"),
            format!("/{head}/./{tail}"),
            format!("{path}?t=1&x=%2F"),
            format!("{path}?"),
            format!("{path};x"),
            format!("{path}%00"),
            format!("{path}%20"),
            format!("http://{address}{path}"),
            format!("http://localhost:{}{path}", address.port()),
            format!("http://evil.example{path}"),
            format!("HTTP://{address}{path}"),
        ] {
            targets.push(spelled);
        }
    }
    targets.push("/__spindle".to_owned());
    targets.push("/__spindle/".to_owned());
    targets.push("/__spindle/ui/panel.html".to_owned());
    targets.push("*".to_owned());
    targets
}

/// No spelling of a run-owned route, under any method, version or `Host`, answers a request
/// without the proxy's secret with the run's state: no 2xx, no event payload, no model, no record,
/// and no reset that empties or plants a row.
#[test]
fn no_spelling_of_a_run_owned_route_answers_without_the_secret() {
    let served = trusting();
    let (method, path) = route(RECORD);
    let host = served.address.to_string();
    let proof = json!({"actor": MEMBER, "caller": {"person_id": PERSON}}).to_string();
    let (status, answer) = raw(
        served.address,
        &request(
            &method,
            &path,
            "HTTP/1.1",
            Some(&host),
            &[(HEADER, &proof), (SECRET_HEADER, SECRET)],
            &json!({"visit_id": VISIT}).to_string(),
        ),
    );
    assert_eq!(status, 202, "the proven member records a visit: {answer}");
    // The routes probed answer the secret, so a refusal below is the guard and not a dead path.
    assert_eq!(
        with_secret(&served, "GET", "/api/events", &[])["events"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let topology = with_secret(&served, "GET", "/__spindle/topology", &[]);
    assert_ne!(topology, Value::Null);
    let world = json!({"rows": [{
        "entity": VISIT_ENTITY,
        "identity": PLANTED,
        "state": "Recorded",
        "fields": {"person_id": FORGED},
    }]})
    .to_string();
    let wrong_secret = SECRET.replace('5', "6");
    let lowered = SECRET.to_uppercase();
    let mut leaks = Vec::new();
    let mut probes = 0;
    for target in targets(served.address) {
        for method in [
            "GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "TRACE", "get", "post",
            "Get",
        ] {
            for version in ["HTTP/1.1", "HTTP/1.0"] {
                for (host, head) in [
                    (Some(host.as_str()), vec![]),
                    (None, vec![]),
                    (Some("localhost"), vec![]),
                    (Some(host.as_str()), vec![(HEADER, proof.as_str())]),
                    (Some(host.as_str()), vec![(SECRET_HEADER, wrong_secret.as_str())]),
                    (Some(host.as_str()), vec![(SECRET_HEADER, lowered.as_str())]),
                    (Some(host.as_str()), vec![(SECRET_HEADER, "")]),
                    (Some(host.as_str()), vec![("X-Forwarded-For", "127.0.0.1")]),
                ] {
                    if version == "HTTP/1.0" && head.len() + usize::from(host.is_none()) > 0 {
                        continue;
                    }
                    let body = if method.eq_ignore_ascii_case("post") {
                        world.as_str()
                    } else {
                        ""
                    };
                    let (status, answer) = raw(
                        served.address,
                        &request(method, &target, version, host, &head, body),
                    );
                    probes += 1;
                    let leaked = (200..300).contains(&status)
                        || answer.contains(PERSON)
                        || answer.contains(VISIT)
                        || answer.contains(AUDITOR)
                        || answer.contains("porch.visit.VisitRecorded");
                    if leaked {
                        leaks.push(format!(
                            "{method} {target} {version} host={host:?} {head:?} -> {}",
                            answer.lines().take(1).collect::<String>()
                        ));
                    }
                }
            }
        }
    }
    let events = with_secret(&served, "GET", "/api/events", &[])["events"].clone();
    let (method, path) = route(LOG);
    let auditor = json!({"actor": AUDITOR}).to_string();
    let rows = with_secret(&served, &method, &path, &[(HEADER, &auditor)])["rows"].clone();
    assert!(
        leaks.is_empty() && events.as_array().map(Vec::len) == Some(1) && !rows.to_string().contains(FORGED),
        "{} of {probes} probes without the secret answered state: {leaks:#?}; events now {events}; \
         visits now {rows}",
        leaks.len()
    );
    assert!(probes > 3_000, "{probes}");
}

/// A pipelined second request on one connection without the secret does not ride on the first
/// request's secret.
#[test]
fn a_pipelined_request_carries_no_secret_from_the_one_before() {
    let served = trusting();
    let host = served.address.to_string();
    let mut both = format!(
        "GET /api/model HTTP/1.1\r\nHost: {host}\r\n{SECRET_HEADER}: {SECRET}\r\n\r\n"
    )
    .into_bytes();
    both.extend_from_slice(
        format!("GET /api/events HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes(),
    );
    let (status, answer) = raw(served.address, &both);
    assert_eq!(status, 200, "{answer}");
    let second = answer
        .match_indices("HTTP/1.1 ")
        .nth(1)
        .map(|(at, _)| &answer[at..])
        .unwrap_or_default();
    assert!(
        second.starts_with("HTTP/1.1 403"),
        "the second request answered {:?}",
        second.lines().next()
    );
}
