//! `run` serving HTTP with `--trusted-caller-header <name>`: each request proves its caller in
//! that header, which a proxy in front of the ingress sets, as one JSON object
//! `{"actor": <qualified actor>, "caller": {<attribute>: <value>}}`. A command that reads its
//! caller runs for the caller the request proves, a read-granted view is read as it, and a request
//! without a valid proof is refused ESS's standard `403` naming no actor, writing nothing.
//!
//! The fixture `tests/fixtures/trusted-caller` declares one component, `front`, reached over the
//! network. The actor `Member` carries `person_id` and may record a visit; `RecordVisit` records
//! the caller's `person_id` on the visit and in the `VisitRecorded` payload. The actor `Auditor`
//! carries no attribute and alone may read `VisitLog`, which makes that view read-granted.

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
const SECRET: &str = "s3cret-proxy-token-7f2a";
const MEMBER: &str = "porch.visit.Member";
const AUDITOR: &str = "porch.visit.Auditor";
const RECORD: &str = "porch.visit.RecordVisit";
const LOG: &str = "porch.visit.VisitLog";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const OTHER: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
const SECOND: &str = "7c1a2b3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

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

/// A directory holding a secret file with `contents`, and the file's path.
fn secret_file(contents: &str) -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("proxy-secret");
    std::fs::write(&path, contents).expect("the secret file is written");
    let path = path.to_str().expect("a UTF-8 path").to_owned();
    (directory, path)
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: mpsc::Receiver<String>,
    /// The secret file the run reads, and its path, kept until the run stops.
    secret: Option<(tempfile::TempDir, String)>,
}

impl Served {
    /// Stops the run and answers everything it printed on stderr.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end once the run stops")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on the fixture and laptop with `arguments` added; `Err` holds the exit code, stdout and
/// stderr of a run that stopped without listening.
fn serve(arguments: &[&str]) -> Result<Served, (Option<i32>, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
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
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let mut rest = String::new();
        if line.is_empty() {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
    });
    let Ok((line, rest)) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let stderr = stderr_read
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end");
        return Err((status.code(), rest, stderr));
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served {
        child,
        address,
        stderr: stderr_read,
        secret: None,
    })
}

/// `run` trusting [`HEADER`], its proxy's secret [`SECRET`] read from a file that ends in a
/// newline.
fn trusting() -> Served {
    trusting_with(&[])
}

/// [`trusting`], `extra` arguments added.
fn trusting_with(extra: &[&str]) -> Served {
    let (directory, path) = secret_file(&format!("{SECRET}\n"));
    let mut arguments = vec![
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        &path,
    ];
    arguments.extend_from_slice(extra);
    let mut served = serve(&arguments)
        .unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"));
    served.secret = Some((directory, path));
    served
}

/// One HTTP/1.1 exchange on its own connection, each of `headers` sent as given: the status and
/// the JSON body.
fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// One request for `operation` with `headers` and the proxy's secret, as the trusted proxy sends
/// it.
fn call(
    served: &Served,
    operation: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let mut sent = headers.to_vec();
    sent.push((SECRET_HEADER, SECRET));
    bare(served, operation, &sent, body)
}

/// One request for `operation` with `headers` alone: no secret unless they carry one.
fn bare(
    served: &Served,
    operation: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let (method, path) = route(operation);
    exchange(served.address, &method, &path, headers, body)
}

fn member(person: &str) -> String {
    json!({"actor": MEMBER, "caller": {"person_id": person}}).to_string()
}

/// Every event published, read through the console as operator tooling behind the proxy reads it:
/// with the proxy's secret and no caller.
fn events(served: &Served) -> Value {
    let (status, events) = exchange(
        served.address,
        "GET",
        "/api/events",
        &[(SECRET_HEADER, SECRET)],
        None,
    );
    assert_eq!(status, 200, "{events}");
    events["events"].clone()
}

fn not_proven() -> Value {
    json!({"refused": "not granted", "actor": null})
}

#[test]
fn a_command_reading_the_caller_runs_for_the_caller_the_request_proves() {
    let served = trusting();
    let proof = member(PERSON);
    let (status, recorded) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(status, 202, "{recorded}");
    assert_eq!(recorded["outcome"], "recorded", "{recorded}");
    assert_eq!(
        recorded["published"][0]["payload"]["person_id"], PERSON,
        "{{caller: person_id}} reads the caller the header proves: {recorded}"
    );
    let proof = member(OTHER);
    let (status, recorded) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": SECOND})),
    );
    assert_eq!(status, 202, "{recorded}");
    assert_eq!(
        recorded["published"][0]["payload"]["person_id"], OTHER,
        "each request proves its own caller; none is fixed for the run: {recorded}"
    );
    let committed = events(&served);
    assert_eq!(
        committed.as_array().map(|events| events
            .iter()
            .map(|event| event["payload"]["person_id"].clone())
            .collect::<Vec<_>>()),
        Some(vec![json!(PERSON), json!(OTHER)]),
        "both outcomes are committed, each for its request's caller: {committed}"
    );
}

#[test]
fn a_request_without_the_header_is_refused_and_writes_nothing() {
    let served = trusting();
    let (status, body) = call(&served, RECORD, &[], Some(&json!({"visit_id": VISIT})));
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        not_proven(),
        "ESS's standard refusal for a request authenticated as no caller"
    );
    let (status, body) = call(&served, LOG, &[], None);
    assert_eq!(status, 403, "a view is refused too: {body}");
    assert_eq!(body, not_proven());
    assert_eq!(events(&served), json!([]), "nothing is published");
}

#[test]
fn a_header_that_proves_no_caller_of_the_model_is_refused() {
    let served = trusting();
    let undeclared = json!({"actor": "porch.visit.Stranger", "caller": {}}).to_string();
    let missing = json!({"actor": MEMBER, "caller": {}}).to_string();
    let mistyped = json!({"actor": MEMBER, "caller": {"person_id": 7}}).to_string();
    let unknown = json!({"actor": MEMBER, "caller": {"person_id": PERSON, "rank": 1}}).to_string();
    let no_actor = json!({"caller": {"person_id": PERSON}}).to_string();
    let extra =
        json!({"actor": MEMBER, "caller": {"person_id": PERSON}, "as": AUDITOR}).to_string();
    let proof = member(PERSON);
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("not JSON", vec![(HEADER, "Member")]),
        ("not an object", vec![(HEADER, "[\"porch.visit.Member\"]")]),
        ("no actor", vec![(HEADER, &no_actor)]),
        ("an undeclared actor", vec![(HEADER, &undeclared)]),
        ("a required attribute left out", vec![(HEADER, &missing)]),
        ("an attribute of the wrong type", vec![(HEADER, &mistyped)]),
        ("an undeclared attribute", vec![(HEADER, &unknown)]),
        ("a member beside actor and caller", vec![(HEADER, &extra)]),
        (
            "the header given twice",
            vec![(HEADER, &proof), (HEADER, &proof)],
        ),
    ];
    for (case, headers) in &cases {
        let (status, body) = call(&served, RECORD, headers, Some(&json!({"visit_id": VISIT})));
        assert_eq!(status, 403, "{case}: {body}");
        assert_eq!(body, not_proven(), "{case}");
    }
    assert_eq!(events(&served), json!([]), "nothing is published");
}

#[test]
fn a_read_granted_view_is_read_as_the_caller_the_request_proves() {
    let served = trusting();
    let proof = member(PERSON);
    let (status, body) = call(&served, LOG, &[(HEADER, &proof)], None);
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({"refused": "not granted", "actor": MEMBER}),
        "a member may not read the auditor's log"
    );
    let auditor = json!({"actor": AUDITOR}).to_string();
    let (status, body) = call(&served, LOG, &[(HEADER, &auditor)], None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"rows": []}), "{body}");
}

#[test]
fn the_header_value_is_never_printed() {
    let served = trusting();
    let marker = "c0ffee00-0000-4000-8000-00000000beef";
    let proof = member(marker);
    let (status, body) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(status, 202, "{body}");
    let malformed = format!("{{\"actor\": \"{marker}\"");
    let (status, body) = call(
        &served,
        RECORD,
        &[(HEADER, &malformed)],
        Some(&json!({"visit_id": SECOND})),
    );
    assert_eq!(status, 403, "{body}");
    let stderr = served.stop();
    assert!(
        !stderr.contains(marker),
        "the proof is not logged: {stderr}"
    );
    let line = stderr
        .lines()
        .find(|line| line.contains("--trusted-caller-header"))
        .unwrap_or_else(|| panic!("a line naming the trusted header at startup: {stderr}"));
    assert!(line.contains(HEADER), "{line}");
    assert!(line.contains("proxy"), "{line}");
}

#[test]
fn the_header_and_a_fixed_actor_are_refused_together() {
    let caller = json!({"person_id": PERSON}).to_string();
    let (_directory, path) = secret_file(SECRET);
    let Err((code, stdout, stderr)) = serve(&[
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        &path,
        "--fixed-actor",
        MEMBER,
        "--fixed-caller",
        &caller,
    ]) else {
        panic!("run listened with both a trusted caller header and a fixed actor");
    };
    assert_eq!(code, Some(1), "{stderr}");
    assert!(!stdout.contains("listening"), "{stdout}");
    assert!(stderr.contains("--trusted-caller-header"), "{stderr}");
    assert!(stderr.contains("--fixed-actor"), "{stderr}");
}

#[test]
fn a_header_name_that_is_no_http_token_is_refused_at_start() {
    let (_directory, path) = secret_file(SECRET);
    let Err((code, stdout, stderr)) = serve(&[
        "--trusted-caller-header",
        "X Caller",
        "--trusted-caller-secret",
        &path,
    ]) else {
        panic!("run listened with a header name holding a space");
    };
    assert_ne!(code, Some(0), "{stderr}");
    assert!(!stdout.contains("listening"), "{stdout}");
    assert!(stderr.contains("X Caller"), "{stderr}");
}

#[test]
fn without_the_option_the_header_proves_nothing() {
    let served = serve(&[]).unwrap_or_else(|stopped| panic!("run stopped: {stopped:?}"));
    let proof = member(PERSON);
    let (status, body) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": VISIT})),
    );
    // The command a grant names is refused as ESS refuses a request authenticated as no caller,
    // before the ingress names the attributes it reads.
    assert_eq!(status, 403, "{body}");
    assert_eq!(body, json!({"refused": "not granted", "actor": null}));
    assert_eq!(events(&served), json!([]), "nothing is published");
}

#[test]
fn the_option_is_refused_where_nothing_is_served() {
    let (_directory, path) = secret_file(SECRET);
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            "crates/spindle-cli/tests/fixtures/adversary-outcome-scoped-refusal-stdin",
            "--environment",
            LAPTOP,
            "--trusted-caller-header",
            HEADER,
            "--trusted-caller-secret",
            &path,
        ])
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("--trusted-caller-header"), "{stderr}");
}

#[test]
fn the_header_without_a_readable_secret_exits_1() {
    let (_empty, empty) = secret_file("\n");
    let (_directory, path) = secret_file(SECRET);
    let missing = format!("{path}.absent");
    let cases: Vec<(&str, Vec<&str>)> = vec![
        (
            "no --trusted-caller-secret",
            vec!["--trusted-caller-header", HEADER],
        ),
        (
            "a secret file that holds only a newline",
            vec![
                "--trusted-caller-header",
                HEADER,
                "--trusted-caller-secret",
                &empty,
            ],
        ),
        (
            "a secret file that does not exist",
            vec![
                "--trusted-caller-header",
                HEADER,
                "--trusted-caller-secret",
                &missing,
            ],
        ),
        (
            "a secret without the caller header",
            vec!["--trusted-caller-secret", &path],
        ),
    ];
    for (case, arguments) in &cases {
        let Err((code, stdout, stderr)) = serve(arguments) else {
            panic!("{case}: run listened");
        };
        assert_eq!(code, Some(1), "{case}: {stderr}");
        assert!(!stdout.contains("listening"), "{case}: {stdout}");
        assert!(
            stderr.contains("--trusted-caller-secret"),
            "{case}: the option is named: {stderr}"
        );
    }
}

#[test]
fn a_valid_caller_without_the_proxy_secret_is_refused_and_writes_nothing() {
    let served = trusting();
    let proof = member(PERSON);
    let short = &SECRET[..SECRET.len() - 1];
    let wrong = SECRET.replace('7', "8");
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("no secret", vec![(HEADER, &proof)]),
        (
            "a wrong secret of the same length",
            vec![(HEADER, &proof), (SECRET_HEADER, &wrong)],
        ),
        (
            "a shorter secret",
            vec![(HEADER, &proof), (SECRET_HEADER, short)],
        ),
        (
            "the secret given twice",
            vec![
                (HEADER, &proof),
                (SECRET_HEADER, SECRET),
                (SECRET_HEADER, SECRET),
            ],
        ),
        (
            "the secret as the caller header's value",
            vec![(HEADER, SECRET), (SECRET_HEADER, &proof)],
        ),
    ];
    for (case, headers) in &cases {
        let (status, body) = bare(&served, RECORD, headers, Some(&json!({"visit_id": VISIT})));
        assert_eq!(status, 403, "{case}: {body}");
        assert_eq!(body, not_proven(), "{case}");
        let (status, body) = bare(&served, LOG, headers, None);
        assert_eq!(status, 403, "{case}, a view: {body}");
        assert_eq!(body, not_proven(), "{case}, a view");
    }
    assert_eq!(events(&served), json!([]), "nothing is published");
    let (status, recorded) = bare(
        &served,
        RECORD,
        &[(HEADER, &proof), (SECRET_HEADER, SECRET)],
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(
        status, 202,
        "the right secret proves the caller: {recorded}"
    );
    assert_eq!(
        recorded["published"][0]["payload"]["person_id"], PERSON,
        "{recorded}"
    );
}

#[test]
fn the_secret_is_never_printed() {
    let served = trusting();
    let proof = member(PERSON);
    let wrong = SECRET.replace('7', "8");
    let (status, body) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(status, 202, "{body}");
    let (status, body) = bare(
        &served,
        RECORD,
        &[(HEADER, &proof), (SECRET_HEADER, &wrong)],
        Some(&json!({"visit_id": SECOND})),
    );
    assert_eq!(status, 403, "{body}");
    let path = served
        .secret
        .as_ref()
        .map(|(_, path)| path.clone())
        .expect("the run reads a secret file");
    let stderr = served.stop();
    assert!(
        !stderr.contains(SECRET),
        "the secret is not printed: {stderr}"
    );
    assert!(
        !stderr.contains(&wrong),
        "a wrong secret is not printed: {stderr}"
    );
    let line = stderr
        .lines()
        .find(|line| line.contains("--trusted-caller-secret"))
        .unwrap_or_else(|| panic!("a line naming the secret file at startup: {stderr}"));
    assert!(
        line.contains(&path),
        "the file is named by its path: {line}"
    );
    assert!(line.contains(SECRET_HEADER), "{line}");
}

/// The console and `/__spindle/` serve operator tooling behind the proxy: on a run whose requests
/// prove their caller each answers a request carrying the proxy's secret, with no caller header,
/// and refuses one without it, or with another, as a request that proves no caller, doing nothing.
#[test]
fn the_console_and_reserved_routes_answer_the_proxy_secret_alone() {
    let served = trusting_with(&["--observe"]);
    let proof = member(PERSON);
    let (status, body) = call(
        &served,
        RECORD,
        &[(HEADER, &proof)],
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(status, 202, "{body}");
    let wrong = SECRET.replace('7', "8");
    let world = json!({"rows": []});
    let routes: [(&str, &str, Option<&Value>); 5] = [
        ("GET", "/api/events", None),
        ("GET", "/api/model", None),
        ("POST", "/api/reset", Some(&world)),
        ("GET", "/__spindle/requests", None),
        ("GET", "/__spindle/metrics", None),
    ];
    for (method, path, body) in routes {
        for (case, headers) in [
            ("no secret", vec![]),
            ("no secret, a valid caller", vec![(HEADER, proof.as_str())]),
            ("a wrong secret", vec![(SECRET_HEADER, wrong.as_str())]),
            (
                "the secret twice",
                vec![(SECRET_HEADER, SECRET), (SECRET_HEADER, SECRET)],
            ),
        ] {
            let (status, answer) = exchange(served.address, method, path, &headers, body);
            assert_eq!(status, 403, "{method} {path}, {case}: {answer}");
            assert_eq!(answer, not_proven(), "{method} {path}, {case}");
        }
    }
    assert_eq!(
        events(&served).as_array().map(Vec::len),
        Some(1),
        "the secret alone reads the events, and no refused reset emptied them"
    );
    let (status, answer) = exchange(
        served.address,
        "GET",
        "/__spindle/requests",
        &[(SECRET_HEADER, SECRET)],
        None,
    );
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = exchange(
        served.address,
        "POST",
        "/api/reset",
        &[(SECRET_HEADER, SECRET)],
        Some(&world),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(events(&served), json!([]), "the reset with the secret ran");
}

/// A proof names each member once at every depth: one that names a member twice, under any
/// spelling of its name, proves no caller, since which value counts depends on the parser.
#[test]
fn a_proof_naming_a_member_twice_at_any_depth_proves_no_caller() {
    let served = trusting();
    let cases = [
        (
            "caller twice",
            format!(
                "{{\"actor\": \"{MEMBER}\", \"caller\": {{\"person_id\": \"{OTHER}\"}}, \
                 \"caller\": {{\"person_id\": \"{PERSON}\"}}}}"
            ),
        ),
        (
            "an attribute twice",
            format!(
                "{{\"actor\": \"{MEMBER}\", \"caller\": {{\"person_id\": \"{OTHER}\", \
                 \"person_id\": \"{PERSON}\"}}}}"
            ),
        ),
        (
            "actor twice, once escaped",
            format!(
                "{{\"actor\": \"{AUDITOR}\", \"\\u0061ctor\": \"{MEMBER}\", \
                 \"caller\": {{\"person_id\": \"{PERSON}\"}}}}"
            ),
        ),
    ];
    for (case, proof) in &cases {
        let (status, body) = call(
            &served,
            RECORD,
            &[(HEADER, proof)],
            Some(&json!({"visit_id": VISIT})),
        );
        assert_eq!(status, 403, "{case}: {body}");
        assert_eq!(body, not_proven(), "{case}");
    }
    assert_eq!(events(&served), json!([]), "nothing is published");
}

/// The transport-free router, which a host with no socket answers through, is handed a request
/// value with no header: on a surface whose requests prove their caller it proves none, so every
/// command and view is refused, and nothing is run as a default caller.
#[test]
fn the_router_refuses_every_request_where_the_caller_is_proven_by_a_header() {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the trusted-caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::proving_callers(
        &ir,
        &[COMPONENT.to_owned()],
        None,
        HEADER,
        SECRET,
    )
    .expect("the fixture's surface is routed")
    .with_console(spindle_ingress::HTTP_LOOPBACK)
    .expect("the console is served")
    .observing(16);
    assert_eq!(surface.caller_header(), Some("x-spindle-caller"));
    assert!(
        surface.unserved().is_empty(),
        "a command that reads its caller is served where each request proves one"
    );
    let environment = spindle_capability::Environment::parse(
        &std::fs::read_to_string(root().join(LAPTOP)).expect("the laptop environment is read"),
    )
    .expect("the laptop environment parses");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut host = spindle_host::Host::new(
        ir,
        &environment,
        std::sync::Arc::new(spindle_host::ManualClock::new()),
        state.path(),
    )
    .expect("the host admits the fixture");
    for operation in [RECORD, LOG] {
        let (method, path) = route(operation);
        let body = if operation == RECORD {
            json!({"visit_id": VISIT}).to_string().into_bytes()
        } else {
            Vec::new()
        };
        let routed = spindle_ingress::route(
            &surface,
            &mut host,
            spindle_ingress::Request {
                method,
                path,
                query: String::new(),
                content_type: Some("application/json".to_owned()),
                body,
            },
        );
        assert_eq!(routed.response.status, 403, "{operation}");
        assert_eq!(
            serde_json::from_str::<Value>(&routed.response.body).expect("a JSON body"),
            not_proven(),
            "{operation}"
        );
    }
    for (method, path) in [
        ("GET", "/api/events"),
        ("GET", "/api/model"),
        ("POST", "/api/reset"),
        ("GET", "/__spindle/requests"),
        ("GET", "/__spindle/metrics"),
    ] {
        let routed = spindle_ingress::route(
            &surface,
            &mut host,
            spindle_ingress::Request {
                method: method.to_owned(),
                path: path.to_owned(),
                query: String::new(),
                content_type: None,
                body: Vec::new(),
            },
        );
        assert_eq!(
            (
                routed.response.status,
                serde_json::from_str::<Value>(&routed.response.body).ok()
            ),
            (403, Some(not_proven())),
            "{method} {path}: no request value carries the proxy's secret"
        );
    }
    assert!(
        host.published().expect("the events read").is_empty(),
        "nothing is published"
    );
}
