//! Adversary pass 1 on `story:http-caller-proves-actor` (wave 19 U2): a run serving HTTP with
//! `--trusted-caller-header` is attacked for a request that reaches the model, or the state, with
//! no proof, or with a proof the contract does not admit.
//!
//! The fixture is the unit's own, `tests/fixtures/trusted-caller`: `Member` (with `person_id`) may
//! record a visit, `Auditor` alone may read `VisitLog`.

use std::io::{BufRead, BufReader, Read, Write};
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
const SECRET: &str = "adv1-proxy-token-91c4";
const MEMBER: &str = "porch.visit.Member";
const AUDITOR: &str = "porch.visit.Auditor";
const RECORD: &str = "porch.visit.RecordVisit";
const LOG: &str = "porch.visit.VisitLog";
const VISIT_ENTITY: &str = "porch.visit.Visit";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const FORGED: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
const SECOND: &str = "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

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

/// `run` trusting [`HEADER`] with the proxy's secret [`SECRET`], `extra` arguments added.
fn trusting(extra: &[&str]) -> Served {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("proxy-secret");
    std::fs::write(&path, format!("{SECRET}\n")).expect("the secret file is written");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(["--trusted-caller-header", HEADER, "--trusted-caller-secret"])
        .arg(&path)
        .args(extra)
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
    let line = receiver
        .recv_timeout(STARTUP)
        .unwrap_or_else(|_| panic!("run printed no line within {STARTUP:?}"));
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
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

/// Reads one answer from `reader`: the status and the body as text.
fn read_answer(reader: &mut BufReader<TcpStream>) -> (u16, String) {
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .expect("a status line is read");
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {status_line:?}"));
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a header line is read");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().expect("a Content-Length");
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("the body is read");
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// One raw request, `head` the header lines after `Host` (each ending in `\r\n`), on its own
/// connection.
fn raw(address: SocketAddr, method: &str, path: &str, head: &[u8], body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n")
            .into_bytes();
    request.extend_from_slice(head);
    request.extend_from_slice(
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    stream.write_all(&request).expect("the request is written");
    read_answer(&mut BufReader::new(stream))
}

fn headers(pairs: &[(&str, &str)]) -> Vec<u8> {
    pairs
        .iter()
        .flat_map(|(name, value)| format!("{name}: {value}\r\n").into_bytes())
        .collect()
}

fn member(person: &str) -> String {
    json!({"actor": MEMBER, "caller": {"person_id": person}}).to_string()
}

fn auditor() -> String {
    json!({"actor": AUDITOR}).to_string()
}

fn not_proven() -> Value {
    json!({"refused": "not granted", "actor": null})
}

/// The visit log, read as the auditor the trusted proxy proves.
fn visits(served: &Served) -> Value {
    let (method, path) = route(LOG);
    let proof = auditor();
    let (status, body) = raw(
        served.address,
        &method,
        &path,
        &headers(&[(HEADER, &proof), (SECRET_HEADER, SECRET)]),
        "",
    );
    assert_eq!(status, 200, "the auditor reads the log: {body}");
    serde_json::from_str::<Value>(&body).expect("JSON")["rows"].clone()
}

fn record(served: &Served, head: &[u8], visit: &str) -> (u16, String) {
    let (method, path) = route(RECORD);
    raw(
        served.address,
        &method,
        &path,
        head,
        &json!({"visit_id": visit}).to_string(),
    )
}

/// The design's fail-closed rule, on the state a client can write: a request that proves no
/// caller writes nothing. `POST /api/reset` on the same listener carries a world whose rows are
/// written into the stores, so an unproven client plants a visit for any person, which the auditor
/// then reads as recorded. On a run whose requests prove their caller it is refused as any other
/// request without a proof.
#[test]
fn an_unproven_reset_writes_no_row_on_a_run_that_proves_callers() {
    let served = trusting(&[]);
    let world = json!({"rows": [{
        "entity": VISIT_ENTITY,
        "identity": VISIT,
        "state": "Recorded",
        "fields": {"person_id": FORGED},
    }]})
    .to_string();
    let (status, body) = raw(served.address, "POST", "/api/reset", b"", &world);
    let rows = visits(&served);
    assert_eq!(
        (status, rows.clone()),
        (403, json!([])),
        "a request with no caller header and no proxy secret wrote a visit for {FORGED}: \
         reset answered {status} {body}, the auditor now reads {rows}"
    );
}

/// The same rule on what a client can read: `GET /api/events` answers every event every proven
/// caller's command published, payloads and `person_id`s included, to a request that proves no
/// caller, past the read grant that keeps `VisitLog` to the auditor.
#[test]
fn an_unproven_request_reads_no_caller_s_events() {
    let served = trusting(&[]);
    let proof = member(PERSON);
    let (status, body) = record(
        &served,
        &headers(&[(HEADER, &proof), (SECRET_HEADER, SECRET)]),
        VISIT,
    );
    assert_eq!(status, 202, "the proven member records a visit: {body}");
    let (status, body) = raw(served.address, "GET", "/api/events", b"", "");
    assert!(
        status == 403 && !body.contains(PERSON),
        "a request with no caller header and no proxy secret read the events: {status} {body}"
    );
}

/// A proof is one JSON object with the members `actor` and `caller`. One that names `actor` twice
/// is not that object: which actor it proves depends on which member a parser keeps, and the
/// proxy and the ingress need not keep the same one. It proves no caller.
#[test]
fn a_proof_naming_actor_twice_proves_no_caller() {
    let served = trusting(&[]);
    let twice = format!(
        "{{\"actor\": \"{AUDITOR}\", \"actor\": \"{MEMBER}\", \"caller\": {{\"person_id\": \"{PERSON}\"}}}}"
    );
    let (status, body) = record(
        &served,
        &headers(&[(HEADER, &twice), (SECRET_HEADER, SECRET)]),
        VISIT,
    );
    assert_eq!(
        (status, serde_json::from_str::<Value>(&body).ok()),
        (403, Some(not_proven())),
        "a proof naming two actors ran as one of them: {body}"
    );
}

/// Probes the bypasses that hold: header case, split and joined values, empty, non-UTF-8 and
/// oversized values, `caller: null`, HTTP/1.0. Each answers no `2xx`, and nothing is written.
#[test]
fn header_shapes_that_prove_no_caller_write_nothing() {
    let served = trusting(&[]);
    let proof = member(PERSON);
    let joined = format!("{SECRET}, {SECRET}");
    let null_caller = json!({"actor": MEMBER, "caller": null}).to_string();
    let long = format!(
        "{{\"actor\": \"{MEMBER}\", \"caller\": {{\"person_id\": \"{PERSON}\"}}, \"pad\": \"{}\"}}",
        "a".repeat(200_000)
    );
    let mut non_utf8 = format!("{HEADER}: ").into_bytes();
    non_utf8.extend_from_slice(b"{\"actor\": \"porch.visit.Member\xff\"}\r\n");
    non_utf8.extend(headers(&[(SECRET_HEADER, SECRET)]));
    let mut secret_non_utf8 = headers(&[(HEADER, &proof)]);
    secret_non_utf8.extend_from_slice(format!("{SECRET_HEADER}: {SECRET}").as_bytes());
    secret_non_utf8.extend_from_slice(b"\xff\r\n");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "the caller header twice under two cases",
            headers(&[
                ("x-spindle-caller", &proof),
                ("X-SPINDLE-CALLER", &proof),
                (SECRET_HEADER, SECRET),
            ]),
        ),
        (
            "the secret twice under two cases",
            headers(&[
                (HEADER, &proof),
                ("spindle-proxy-secret", SECRET),
                ("SPINDLE-PROXY-SECRET", SECRET),
            ]),
        ),
        (
            "the secret comma-joined with itself",
            headers(&[(HEADER, &proof), (SECRET_HEADER, &joined)]),
        ),
        (
            "an empty secret",
            headers(&[(HEADER, &proof), (SECRET_HEADER, "")]),
        ),
        (
            "the secret folded onto a second line",
            format!("{HEADER}: {proof}\r\n{SECRET_HEADER}: adv1-proxy\r\n -token-91c4\r\n")
                .into_bytes(),
        ),
        ("a caller header that is not UTF-8", non_utf8),
        ("a secret header that is not UTF-8", secret_non_utf8),
        (
            "caller null",
            headers(&[(HEADER, &null_caller), (SECRET_HEADER, SECRET)]),
        ),
        (
            "a 200 kB proof",
            headers(&[(HEADER, &long), (SECRET_HEADER, SECRET)]),
        ),
        (
            "the secret's prefix",
            headers(&[(HEADER, &proof), (SECRET_HEADER, &SECRET[..4])]),
        ),
        (
            "the secret with a byte added",
            headers(&[(HEADER, &proof), (SECRET_HEADER, &format!("{SECRET}x"))]),
        ),
    ];
    for (case, head) in &cases {
        let (status, body) = record(&served, head, SECOND);
        assert!(!(200..300).contains(&status), "{case}: {status} {body}");
    }
    let (method, path) = route(RECORD);
    let mut stream = TcpStream::connect_timeout(&served.address, ANSWER).expect("accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let body = json!({"visit_id": SECOND}).to_string();
    write!(
        stream,
        "{method} {path} HTTP/1.0\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        served.address,
        body.len()
    )
    .expect("written");
    let (status, answer) = read_answer(&mut BufReader::new(stream));
    assert_eq!(status, 403, "HTTP/1.0 without a proof: {answer}");
    assert_eq!(visits(&served), json!([]), "nothing is written");
}

/// Header names are case-insensitive: a proxy that spells either header in another case proves
/// the caller as one that spells it as configured.
#[test]
fn header_names_match_in_any_case() {
    let served = trusting(&[]);
    let proof = member(PERSON);
    let (status, body) = record(
        &served,
        &headers(&[
            ("x-spindle-caller", &proof),
            ("SPINDLE-PROXY-SECRET", SECRET),
        ]),
        VISIT,
    );
    assert_eq!(status, 202, "{body}");
    assert!(body.contains(PERSON), "{body}");
}

/// One kept-alive connection carrying a proven request and then an unproven one: the second
/// proves nothing from the first.
#[test]
fn a_kept_alive_connection_carries_no_proof_from_one_request_to_the_next() {
    let served = trusting(&[]);
    let (method, path) = route(RECORD);
    let proof = member(PERSON);
    let mut stream = TcpStream::connect_timeout(&served.address, ANSWER).expect("accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    for (visit, head) in [
        (VISIT, headers(&[(HEADER, &proof), (SECRET_HEADER, SECRET)])),
        (SECOND, Vec::new()),
    ] {
        let body = json!({"visit_id": visit}).to_string();
        let mut request =
            format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", served.address).into_bytes();
        request.extend_from_slice(&head);
        request.extend_from_slice(
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
        stream.write_all(&request).expect("written");
        let (status, answer) = read_answer(&mut reader);
        if visit == VISIT {
            assert_eq!(status, 202, "{answer}");
        } else {
            assert_eq!(status, 403, "{answer}");
            assert_eq!(
                serde_json::from_str::<Value>(&answer).ok(),
                Some(not_proven())
            );
        }
    }
    let rows = visits(&served);
    assert_eq!(
        rows.as_array().map(Vec::len),
        Some(1),
        "only the proven visit: {rows}"
    );
}
