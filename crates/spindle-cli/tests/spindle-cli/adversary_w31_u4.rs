//! Adversary cases for `run --caller-token <FILE>` (wave 31 U4, `story:verified-caller-token`):
//! the bearer header read once and only there, the token's header and claims beyond the cases the
//! unit lists, the mapping's leeway, keys the set lists for encryption or for another algorithm,
//! the key set's refresh (a failed one, and one that removes every key), the fetch's bounds (a
//! huge, a silent, an unusable set) and the mapping's bounds at start.
//!
//! Every key is made here with `rcgen` (P-256), every token signed here, and every key set served
//! by a listener on loopback inside this test; no key is written to the tree.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "front";
const ISSUER: &str = "https://issuer.test";
const AUDIENCE: &str = "porch";
const LOG: &str = "porch.visit.VisitLog";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";

/// The headers of one request, each name with its value, in order.
type Headers = Vec<(&'static str, String)>;

const STARTUP: Duration = Duration::from_secs(50);
const ANSWER: Duration = Duration::from_secs(10);

/// The top keys of a mapping the unit's own cases use: ES256, no leeway, a refresh every 300 s.
const PLAIN: &str = "algorithms = [\"ES256\"]\nleeway_seconds = 0\nrefresh_seconds = 300";

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

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (at, byte)| {
            group | (u32::from(*byte) << (16 - 8 * at))
        });
        for at in 0..=chunk.len() {
            let index = (group >> (18 - 6 * at)) & 0x3f;
            text.push(char::from(ALPHABET[index as usize]));
        }
    }
    text
}

struct Signer {
    kid: String,
    key: rcgen::KeyPair,
}

impl Signer {
    fn new(kid: &str) -> Self {
        Self {
            kid: kid.to_owned(),
            key: rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).expect("a P-256 key"),
        }
    }

    /// The public key as a JWK, `extra` members added or replaced.
    fn jwk_with(&self, extra: &Value) -> Value {
        let point = self.key.public_key_raw();
        let mut jwk = json!({
            "kty": "EC",
            "crv": "P-256",
            "use": "sig",
            "alg": "ES256",
            "kid": self.kid,
            "x": base64url(&point[1..33]),
            "y": base64url(&point[33..]),
        });
        for (name, value) in extra.as_object().expect("an object") {
            jwk[name] = value.clone();
        }
        jwk
    }

    fn jwk(&self) -> Value {
        self.jwk_with(&json!({}))
    }

    fn header(&self) -> jsonwebtoken::Header {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        header
    }

    fn sign_header(&self, header: &jsonwebtoken::Header, claims: &Value) -> String {
        let key = jsonwebtoken::EncodingKey::from_ec_der(&self.key.serialize_der());
        jsonwebtoken::encode(header, claims, &key).expect("the token is signed")
    }

    fn sign(&self, claims: &Value) -> String {
        self.sign_header(&self.header(), claims)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs()
}

fn claims(role: &str) -> Value {
    json!({
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": now() + 3600,
        "iat": now(),
        "role": role,
        "sub": PERSON,
    })
}

/// What the key set listener answers.
#[derive(Clone)]
enum Answering {
    /// `200` with `{"keys": [..]}`.
    Keys(Vec<Value>),
    /// `200` with this body as it is.
    Body(String),
    /// This status, no body.
    Status(u16),
    /// Nothing: the connection is held open and never answered.
    Silent,
}

struct Jwks {
    address: SocketAddr,
    answering: Arc<Mutex<Answering>>,
}

impl Jwks {
    fn serving(answering: Answering) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("the key set binds");
        let address = listener.local_addr().expect("its address");
        let answering = Arc::new(Mutex::new(answering));
        let served = Arc::clone(&answering);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(ANSWER));
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                let now = served.lock().expect("the answer").clone();
                let (status, body) = match now {
                    Answering::Keys(keys) => (200, json!({"keys": keys}).to_string()),
                    Answering::Body(body) => (200, body),
                    Answering::Status(status) => (status, String::new()),
                    Answering::Silent => {
                        held.push(stream);
                        continue;
                    }
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { address, answering }
    }

    fn url(&self) -> String {
        format!("http://{}/jwks", self.address)
    }

    fn answer(&self, answering: Answering) {
        *self.answering.lock().expect("the answer") = answering;
    }
}

struct Mapping {
    _directory: tempfile::TempDir,
    path: String,
}

impl Mapping {
    /// A mapping whose key set is at `jwks`, its top keys `top`, `tail` added at its end.
    fn with(jwks: &str, top: &str, tail: &str) -> Self {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let path = directory.path().join("caller-token.toml");
        std::fs::write(
            &path,
            format!(
                r#"issuer = "{ISSUER}"
audience = "{AUDIENCE}"
jwks = "{jwks}"
{top}

[actor]
claim = "role"

[actor.values]
member = "porch.visit.Member"
auditor = "porch.visit.Auditor"

[attributes."porch.visit.Member"]
person_id = "sub"
{tail}
"#
            ),
        )
        .expect("the mapping is written");
        let path = path.to_str().expect("a UTF-8 path").to_owned();
        Self {
            _directory: directory,
            path,
        }
    }

    fn new(jwks: &str, top: &str) -> Self {
        Self::with(jwks, top, "")
    }
}

/// A running `run`, its stderr read as it is written; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Arc<Mutex<String>>,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
struct Stopped {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Stopped {
    fn line_with(&self, needles: &[&str]) -> &str {
        self.stderr
            .lines()
            .find(|line| needles.iter().all(|needle| line.contains(needle)))
            .unwrap_or_else(|| panic!("no line names all of {needles:?}: {self:?}"))
    }
}

fn serve(arguments: &[&str]) -> Result<Served, Stopped> {
    serve_on(SPEC, arguments)
}

/// `run` on the fixture `spec` and laptop with `arguments` added.
fn serve_on(spec: &str, arguments: &[&str]) -> Result<Served, Stopped> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP])
        .args(arguments)
        .env_remove("SPINDLE_SETTING_CONSOLE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = child.stderr.take().expect("stderr is piped");
    let written = Arc::new(Mutex::new(String::new()));
    let appended = Arc::clone(&written);
    let (ended, stderr_ended) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
            appended.lock().expect("stderr").push_str(&line);
            line.clear();
        }
        let _ = ended.send(());
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
        let _ = stderr_ended.recv_timeout(ANSWER);
        let stderr = written.lock().expect("stderr").clone();
        return Err(Stopped {
            code: status.code(),
            stdout: rest,
            stderr,
        });
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
        stderr: written,
    })
}

fn token_run(mapping: &Mapping) -> Served {
    serve(&["--caller-token", mapping.path.as_str()])
        .unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

fn stops(arguments: &[&str]) -> Stopped {
    match serve(arguments) {
        Ok(served) => {
            drop(served);
            panic!("run listened with {arguments:?}");
        }
        Err(stopped) => stopped,
    }
}

/// One exchange: `method` `path` with `headers` as given, every one of them, in order.
fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let text = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .write_all(text.as_bytes())
        .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).unwrap_or_else(|_| json!(body))
    };
    (status, body)
}

/// `GET VisitLog` with `headers`, and `?query` where given.
fn read_log(served: &Served, headers: &[(&str, &str)], query: Option<&str>) -> (u16, Value) {
    let (method, path) = route(LOG);
    let path = match query {
        Some(query) => format!("{path}?{query}"),
        None => path,
    };
    exchange(served.address, &method, &path, headers, None)
}

fn with_token(served: &Served, token: &str) -> (u16, Value) {
    let authorization = format!("Bearer {token}");
    read_log(served, &[("Authorization", &authorization)], None)
}

fn answers_within(served: &Served, token: &str, status: u16, within: Duration) -> bool {
    let until = Instant::now() + within;
    loop {
        if with_token(served, token).0 == status {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn the_bearer_token_is_read_once_from_authorization_and_nowhere_else() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), PLAIN);
    let served = token_run(&mapping);
    let auditor = signer.sign(&claims("auditor"));

    assert_eq!(with_token(&served, &auditor).0, 200, "the token verifies");
    for scheme in ["bearer", "BEARER", "bEaReR"] {
        let value = format!("{scheme} {auditor}");
        let (status, body) = read_log(&served, &[("Authorization", &value)], None);
        assert_eq!(
            status, 200,
            "the scheme {scheme} is read in any case: {body}"
        );
    }

    let bearer = format!("Bearer {auditor}");
    let refused: Vec<(&str, Headers, Option<String>)> = vec![
        (
            "the same valid token twice",
            vec![
                ("Authorization", bearer.clone()),
                ("Authorization", bearer.clone()),
            ],
            None,
        ),
        (
            "a valid token beside a second header",
            vec![
                ("Authorization", bearer.clone()),
                ("Authorization", "Bearer x".to_owned()),
            ],
            None,
        ),
        (
            "the token in the query string",
            Vec::new(),
            Some(format!("access_token={auditor}")),
        ),
        (
            "the token in a cookie",
            vec![("Cookie", format!("access_token={auditor}"))],
            None,
        ),
        (
            "the token under another scheme",
            vec![("Authorization", format!("Basic {auditor}"))],
            None,
        ),
        (
            "the token without a scheme",
            vec![("Authorization", auditor.clone())],
            None,
        ),
        (
            "the scheme run into the token",
            vec![("Authorization", format!("Bearer{auditor}"))],
            None,
        ),
        (
            "the scheme alone",
            vec![("Authorization", "Bearer".to_owned())],
            None,
        ),
        (
            "the token in another header",
            vec![("X-Access-Token", auditor.clone())],
            None,
        ),
    ];
    for (case, headers, query) in &refused {
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let (status, body) = read_log(&served, &headers, query.as_deref());
        assert_eq!(status, 401, "{case}: {body}");
        assert_eq!(body, json!({"refused": "unauthenticated"}), "{case}");
    }
}

#[test]
fn a_token_whose_header_or_claims_fail_is_refused_beyond_the_listed_cases() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), PLAIN);
    let served = token_run(&mapping);

    let mut no_kid = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    no_kid.kid = None;
    let by_public_point = {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
        header.kid = Some("k1".to_owned());
        jsonwebtoken::encode(
            &header,
            &claims("auditor"),
            &jsonwebtoken::EncodingKey::from_secret(signer.key.public_key_raw()),
        )
        .expect("signed with the public point as secret")
    };
    let by_public_pem = {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
        header.kid = Some("k1".to_owned());
        jsonwebtoken::encode(
            &header,
            &claims("auditor"),
            &jsonwebtoken::EncodingKey::from_secret(signer.key.public_key_pem().as_bytes()),
        )
        .expect("signed with the public key's PEM as secret")
    };
    let swapped = {
        let auditor = signer.sign(&claims("auditor"));
        let member = signer.sign(&claims("member"));
        let auditor: Vec<&str> = auditor.split('.').collect();
        let member: Vec<&str> = member.split('.').collect();
        format!("{}.{}.{}", auditor[0], member[1], auditor[2])
    };
    let without = |claim: &str| {
        let mut claims = claims("auditor");
        claims.as_object_mut().expect("an object").remove(claim);
        signer.sign(&claims)
    };
    let changed = |claim: &str, value: Value| {
        let mut claims = claims("auditor");
        claims[claim] = value;
        signer.sign(&claims)
    };
    let refused = [
        ("no kid", signer.sign_header(&no_kid, &claims("auditor"))),
        ("HS256 keyed with the public point", by_public_point),
        ("HS256 keyed with the public key's PEM", by_public_pem),
        ("a signature over another payload", swapped),
        ("no exp", without("exp")),
        ("no iss", without("iss")),
        ("no aud", without("aud")),
        ("nbf ten minutes ahead", changed("nbf", json!(now() + 600))),
        (
            "exp as a string",
            changed("exp", json!((now() + 3600).to_string())),
        ),
        (
            "aud an array without the audience",
            changed("aud", json!(["other", "porch2"])),
        ),
        ("aud an empty array", changed("aud", json!([]))),
        (
            "iss with a trailing slash",
            changed("iss", json!(format!("{ISSUER}/"))),
        ),
    ];
    for (case, token) in &refused {
        let (status, body) = with_token(&served, token);
        assert_eq!(status, 401, "{case}: {body}");
    }

    let (status, body) = with_token(&served, &changed("aud", json!(["other", AUDIENCE])));
    assert_eq!(
        status, 200,
        "an aud array naming the audience verifies: {body}"
    );

    for (case, role) in [
        ("a role claim that is an array", json!(["auditor"])),
        ("a role claim that is a number", json!(1)),
        ("a role claim that is an object", json!({"auditor": true})),
        ("a role claim in another case", json!("Auditor")),
    ] {
        let mut claims = claims("auditor");
        claims["role"] = role;
        let (status, body) = with_token(&served, &signer.sign(&claims));
        assert_eq!(status, 403, "{case}: {body}");
        assert_eq!(
            body,
            json!({"refused": "not granted", "actor": null}),
            "{case}"
        );
    }
}

#[test]
fn the_mapping_leeway_bounds_exp_and_nbf() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(
        &jwks.url(),
        "algorithms = [\"ES256\"]\nleeway_seconds = 120\nrefresh_seconds = 300",
    );
    let served = token_run(&mapping);
    let at = |claim: &str, value: u64| {
        let mut claims = claims("auditor");
        claims[claim] = json!(value);
        signer.sign(&claims)
    };
    for (case, token, status) in [
        (
            "exp 90 s past, within the leeway",
            at("exp", now() - 90),
            200,
        ),
        (
            "exp 200 s past, beyond the leeway",
            at("exp", now() - 200),
            401,
        ),
        (
            "nbf 90 s ahead, within the leeway",
            at("nbf", now() + 90),
            200,
        ),
        (
            "nbf 200 s ahead, beyond the leeway",
            at("nbf", now() + 200),
            401,
        ),
    ] {
        let (answered, body) = with_token(&served, &token);
        assert_eq!(answered, status, "{case}: {body}");
    }
}

#[test]
fn a_key_for_encryption_or_for_another_algorithm_verifies_nothing() {
    let signer = Signer::new("k1");
    let encrypting = Signer::new("enc");
    let other_alg = Signer::new("es384");
    let no_use = Signer::new("bare");
    let jwks = Jwks::serving(Answering::Keys(vec![
        signer.jwk(),
        encrypting.jwk_with(&json!({"use": "enc"})),
        other_alg.jwk_with(&json!({"alg": "ES384"})),
        no_use.jwk_with(&json!({"use": null, "alg": null})),
    ]));
    let mapping = Mapping::new(
        &jwks.url(),
        "algorithms = [\"ES256\", \"ES384\"]\nleeway_seconds = 0",
    );
    let served = token_run(&mapping);

    assert_eq!(with_token(&served, &signer.sign(&claims("auditor"))).0, 200);
    assert_eq!(
        with_token(&served, &no_use.sign(&claims("auditor"))).0,
        200,
        "a P-256 key naming no use and no algorithm verifies an ES256 token"
    );
    for (case, key) in [
        ("a key listed for encryption", &encrypting),
        ("a P-256 key naming ES384", &other_alg),
    ] {
        let (status, body) = with_token(&served, &key.sign(&claims("auditor")));
        assert_eq!(status, 401, "{case}: {body}");
    }
}

#[test]
fn a_refresh_that_fails_keeps_the_keys_and_warns_naming_the_url() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), "algorithms = [\"ES256\"]\nrefresh_seconds = 1");
    let served = token_run(&mapping);
    let auditor = signer.sign(&claims("auditor"));
    assert_eq!(with_token(&served, &auditor).0, 200);

    jwks.answer(Answering::Status(500));
    let until = Instant::now() + Duration::from_secs(15);
    while !served
        .stderr
        .lock()
        .expect("stderr")
        .lines()
        .any(|line| line.contains(&jwks.url()) && line.contains("could not be refreshed"))
    {
        assert!(
            Instant::now() < until,
            "no warning names the key set within 15 s: {}",
            served.stderr.lock().expect("stderr")
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(
        with_token(&served, &auditor).0,
        200,
        "the keys fetched last still verify"
    );
    let written = served.stderr.lock().expect("stderr").clone();
    assert!(
        !written.contains(&auditor),
        "no token is printed: {written}"
    );
}

/// Acceptance `keys_rotate_without_a_restart`: "a removed key no longer does". An issuer that
/// removes its only key publishes a set with no key; the run treats that as a failed refresh and
/// keeps verifying the removed key for as long as it runs.
#[test]
fn a_key_removed_by_emptying_the_set_no_longer_verifies() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), "algorithms = [\"ES256\"]\nrefresh_seconds = 1");
    let served = token_run(&mapping);
    let auditor = signer.sign(&claims("auditor"));
    assert_eq!(with_token(&served, &auditor).0, 200);

    jwks.answer(Answering::Keys(Vec::new()));
    assert!(
        answers_within(&served, &auditor, 401, Duration::from_secs(8)),
        "a key the issuer removed from its set, leaving it empty, still verifies: {}",
        served.stderr.lock().expect("stderr")
    );
}

#[test]
fn a_key_set_too_large_too_slow_or_without_a_usable_key_stops_the_run() {
    let signer = Signer::new("k1");
    let padding = "x".repeat(1024 * 1024 + 16);
    let huge = Jwks::serving(Answering::Body(
        json!({"keys": [signer.jwk()], "padding": padding}).to_string(),
    ));
    let mapping = Mapping::new(&huge.url(), PLAIN);
    let stopped = stops(&["--caller-token", &mapping.path]);
    assert_eq!(stopped.code, Some(1), "a key set over 1 MiB: {stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&[&huge.url()]);

    let unusable = Jwks::serving(Answering::Keys(vec![
        signer.jwk_with(&json!({"use": "enc"})),
        signer.jwk_with(&json!({"kid": null})),
        json!({"kty": "oct", "kid": "secret", "k": base64url(b"shared")}),
    ]));
    let mapping = Mapping::new(&unusable.url(), PLAIN);
    let stopped = stops(&["--caller-token", &mapping.path]);
    assert_eq!(
        stopped.code,
        Some(1),
        "a key set with no usable key: {stopped:?}"
    );
    stopped.line_with(&[&unusable.url()]);

    let silent = Jwks::serving(Answering::Silent);
    let mapping = Mapping::new(&silent.url(), PLAIN);
    let started = Instant::now();
    let stopped = stops(&["--caller-token", &mapping.path]);
    assert_eq!(
        stopped.code,
        Some(1),
        "a key set that never answers: {stopped:?}"
    );
    stopped.line_with(&[&silent.url()]);
    assert!(
        started.elapsed() < Duration::from_secs(40),
        "the fetch is bounded: {:?}",
        started.elapsed()
    );
}

#[test]
fn a_mapping_out_of_bounds_exits_1_before_listening() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let url = jwks.url();
    let cases: Vec<(&str, Mapping, &str)> = vec![
        (
            "a leeway over 300 s",
            Mapping::new(&url, "algorithms = [\"ES256\"]\nleeway_seconds = 301"),
            "leeway_seconds",
        ),
        (
            "a refresh of 0 s",
            Mapping::new(&url, "algorithms = [\"ES256\"]\nrefresh_seconds = 0"),
            "refresh_seconds",
        ),
        (
            "a refresh over a day",
            Mapping::new(&url, "algorithms = [\"ES256\"]\nrefresh_seconds = 86401"),
            "refresh_seconds",
        ),
        (
            "a symmetric algorithm",
            Mapping::new(&url, "algorithms = [\"ES256\", \"HS256\"]"),
            "HS256",
        ),
        (
            "http to a host off loopback",
            Mapping::new("http://192.0.2.1/jwks", PLAIN),
            "http://192.0.2.1/jwks",
        ),
        (
            "attributes for an actor no role maps to",
            Mapping::with(
                &url,
                PLAIN,
                "\n[attributes.\"porch.visit.Ghost\"]\nx = \"sub\"",
            ),
            "porch.visit.Ghost",
        ),
    ];
    for (case, mapping, named) in &cases {
        let stopped = stops(&["--caller-token", &mapping.path]);
        assert_eq!(stopped.code, Some(1), "{case}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{case}: {stopped:?}");
        stopped.line_with(&["--caller-token", named]);
    }

    let mapping = Mapping::new(&url, PLAIN);
    let stopped = stops(&[
        "--caller-token",
        &mapping.path,
        "--fixed-actor",
        "porch.visit.Auditor",
    ]);
    assert_eq!(stopped.code, Some(1), "with --fixed-actor: {stopped:?}");
    stopped.line_with(&["--caller-token", "--fixed-actor"]);
}

// Round 2: the correction round's F1 (a refresh that parses replaces the keys), F2 (a token that
// maps to no caller reads none of the run's own routes but the probes) and F3 (the mapping is
// checked against the specification at start).

const RECORD_VISIT: &str = "porch.visit.RecordVisit";
const A_VISIT: &str = "7d4f0b36-5e8c-4a9d-9f3b-4c5d6e7f8091";

fn token_run_with(mapping: &Mapping, extra: &[&str]) -> Served {
    let mut arguments = vec!["--caller-token", mapping.path.as_str()];
    arguments.extend_from_slice(extra);
    serve(&arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

/// `GET path` with `token` as its bearer token.
fn get_as(served: &Served, path: &str, token: &str) -> (u16, Value) {
    let authorization = format!("Bearer {token}");
    exchange(
        served.address,
        "GET",
        path,
        &[("Authorization", &authorization)],
        None,
    )
}

/// The lines of the run's stderr holding `needle` so far.
fn warned(served: &Served, needle: &str) -> usize {
    served
        .stderr
        .lock()
        .expect("stderr")
        .lines()
        .filter(|line| line.contains(needle))
        .count()
}

/// Waits at most 8 s for a line holding `needle` beyond the `before` there were.
fn warns_again(served: &Served, needle: &str, before: usize) -> bool {
    let until = Instant::now() + Duration::from_secs(8);
    while warned(served, needle) <= before {
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

#[test]
fn a_key_set_that_empties_breaks_and_refills_revokes_and_restores_its_keys() {
    let first = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![first.jwk()]));
    let mapping = Mapping::new(&jwks.url(), "algorithms = [\"ES256\"]\nrefresh_seconds = 1");
    let served = token_run(&mapping);
    let by_first = first.sign(&claims("auditor"));
    assert_eq!(with_token(&served, &by_first).0, 200);

    jwks.answer(Answering::Keys(Vec::new()));
    assert!(
        answers_within(&served, &by_first, 401, Duration::from_secs(8)),
        "an emptied set revokes the key"
    );
    assert!(
        warned(&served, "was refreshed") > 0 && warned(&served, &jwks.url()) > 0,
        "the empty refresh warns naming the key set: {}",
        served.stderr.lock().expect("stderr")
    );

    let failed = "could not be refreshed";
    let before = warned(&served, failed);
    jwks.answer(Answering::Body("<html>not a key set</html>".to_owned()));
    assert!(
        warns_again(&served, failed, before),
        "an unparseable refresh warns"
    );
    assert_eq!(
        with_token(&served, &by_first).0,
        401,
        "an unparseable refresh after an empty one keeps the empty set"
    );

    jwks.answer(Answering::Keys(vec![first.jwk()]));
    assert!(
        answers_within(&served, &by_first, 200, Duration::from_secs(8)),
        "a refilled set verifies the key again"
    );
    let before = warned(&served, failed);
    jwks.answer(Answering::Body(r#"{"keys": "none"}"#.to_owned()));
    assert!(
        warns_again(&served, failed, before),
        "a set that is no JWKS warns"
    );
    assert_eq!(
        with_token(&served, &by_first).0,
        200,
        "a refresh that is no JWKS keeps the usable set"
    );

    let reissued = Signer::new("k1");
    jwks.answer(Answering::Keys(vec![reissued.jwk()]));
    let by_reissued = reissued.sign(&claims("auditor"));
    assert!(
        answers_within(&served, &by_reissued, 200, Duration::from_secs(8)),
        "the kid k1 reappearing with another key verifies that key"
    );
    assert_eq!(
        with_token(&served, &by_first).0,
        401,
        "the kid k1 reappearing does not verify the key it named before"
    );
}

#[test]
fn the_runs_own_routes_refuse_a_bad_token_401_before_an_unmapped_one_403() {
    let signer = Signer::new("k1");
    let stranger_key = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), PLAIN);
    let served = token_run_with(&mapping, &["--observe"]);
    let unmapped = signer.sign(&claims("stranger"));
    let unmapped_by_stranger = stranger_key.sign(&claims("stranger"));
    let mut expired = claims("stranger");
    expired["exp"] = json!(now() - 3600);
    let expired = signer.sign(&expired);
    let not_granted = json!({"refused": "not granted", "actor": null});

    for path in [
        "/__spindle/requests",
        "/__spindle/metrics",
        "/__spindle/topology",
        "/__spindle/other",
    ] {
        for (case, token) in [
            ("an unmapped claim under another key", &unmapped_by_stranger),
            ("an unmapped claim, expired", &expired),
        ] {
            let (status, body) = get_as(&served, path, token);
            assert_eq!(status, 401, "{path}, {case}: {body}");
        }
        let (status, body) = get_as(&served, path, &unmapped);
        assert_eq!(status, 403, "{path}: {body}");
        assert_eq!(body, not_granted, "{path}");
    }
    for probe in ["/__spindle/live", "/__spindle/ready", "/__spindle/seed"] {
        for token in [&unmapped, &unmapped_by_stranger, &expired] {
            let (status, body) = get_as(&served, probe, token);
            assert_eq!(status, 200, "{probe} answers whatever the token: {body}");
        }
    }

    let (method, path) = route(RECORD_VISIT);
    let bearer = format!("Bearer {unmapped}");
    let (status, body) = exchange(
        served.address,
        &method,
        &path,
        &[("Authorization", &bearer)],
        Some(&json!({"visit_id": A_VISIT})),
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(body, not_granted);

    for role in ["auditor", "member"] {
        let mapped = signer.sign(&claims(role));
        for path in [
            "/__spindle/requests",
            "/__spindle/metrics",
            "/__spindle/topology",
        ] {
            let (status, body) = get_as(&served, path, &mapped);
            assert_eq!(status, 200, "a {role} token reads {path}: {body}");
        }
    }
    let (_, log) = get_as(
        &served,
        "/__spindle/requests",
        &signer.sign(&claims("auditor")),
    );
    assert_eq!(
        log["recorded"], 1,
        "only the command's 403 is recorded, no answer under /__spindle/: {log}"
    );
}

/// F2 says a token that maps to no caller reads none of the run's own routes. A token whose role
/// maps to `Member` but which carries no `sub`, or a `sub` that is no `PersonId`, is no caller of
/// the model: a command answers it `403` naming no actor. Under `/__spindle/` it still reads the
/// request log, the metrics and the topology.
#[test]
fn a_token_whose_caller_does_not_bind_reads_none_of_the_runs_own_routes() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = Mapping::new(&jwks.url(), PLAIN);
    let served = token_run_with(&mapping, &["--observe"]);
    let not_granted = json!({"refused": "not granted", "actor": null});
    let mut no_sub = claims("member");
    no_sub.as_object_mut().expect("an object").remove("sub");
    let mut wrong_sub = claims("member");
    wrong_sub["sub"] = json!(7);

    for (case, claims) in [("no sub", no_sub), ("a sub of the wrong type", wrong_sub)] {
        let token = signer.sign(&claims);
        let (method, path) = route(RECORD_VISIT);
        let bearer = format!("Bearer {token}");
        let (status, body) = exchange(
            served.address,
            &method,
            &path,
            &[("Authorization", &bearer)],
            Some(&json!({"visit_id": A_VISIT})),
        );
        assert_eq!(status, 403, "{case}, a command: {body}");
        assert_eq!(body, not_granted, "{case}: a command reads it as no caller");
        for path in [
            "/__spindle/requests",
            "/__spindle/metrics",
            "/__spindle/topology",
        ] {
            let (status, body) = get_as(&served, path, &token);
            assert_eq!(
                status, 403,
                "{case}: {path} answers a token of no caller: {body}"
            );
            assert_eq!(body, not_granted, "{case}: {path}");
        }
    }
}

#[test]
fn a_mapping_naming_another_actors_attribute_or_an_unqualified_actor_exits_1() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let url = jwks.url();
    let table = |values: &str, attributes: &str| {
        format!(
            "issuer = \"{ISSUER}\"\naudience = \"{AUDIENCE}\"\njwks = \"{url}\"\n{PLAIN}\n\n\
             [actor]\nclaim = \"role\"\n\n[actor.values]\n{values}\n\n{attributes}\n"
        )
    };
    let cases = [
        (
            "the member's attribute mapped for the auditor",
            table(
                "member = \"porch.visit.Member\"\nauditor = \"porch.visit.Auditor\"",
                "[attributes.\"porch.visit.Auditor\"]\nperson_id = \"sub\"",
            ),
            vec!["porch.visit.Auditor", "person_id"],
        ),
        (
            "an actor named without its domain",
            table("member = \"Member\"", ""),
            vec!["Member"],
        ),
        (
            "an actor named in another case",
            table("member = \"porch.visit.member\"", ""),
            vec!["porch.visit.member"],
        ),
        (
            "a domain named as an actor",
            table("member = \"porch.visit\"", ""),
            vec!["porch.visit"],
        ),
    ];
    let mapping = Mapping::new(&url, PLAIN);
    for (at, (case, text, named)) in cases.into_iter().enumerate() {
        let path = Path::new(&mapping.path).with_file_name(format!("mapping-{at}.toml"));
        std::fs::write(&path, text).expect("the mapping is written");
        let path = path.to_str().expect("a UTF-8 path").to_owned();
        let stopped = stops(&["--caller-token", &path]);
        assert_eq!(stopped.code, Some(1), "{case}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{case}: {stopped:?}");
        let mut needles = vec!["--caller-token", path.as_str()];
        needles.extend(named);
        stopped.line_with(&needles);
    }
}

// Round 3: G1 (a mapped caller the model does not bind is no caller) and G3 (a mapped actor's
// required attribute mapped from no claim stops the run), on the fixture `adversary-fixed-caller`:
// `Librarian` carries `branch` (an enum, North or South), `level` (Integer) and `nickname`
// (Optional<String>) and may add and shelve books and read `Catalogue`; `Visitor` carries
// nothing and may add books.

const SHELF: &str = "crates/spindle-cli/tests/fixtures/adversary-fixed-caller";
const SHELF_COMPONENT: &str = "stacks";

fn shelf_route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SHELF))
        .expect("the shelf fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[SHELF_COMPONENT.to_owned()], None)
        .expect("the shelf surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// A shelf mapping: `librarian` and `visitor` mapped, and `attributes` as the librarian's table.
fn shelf_mapping(jwks: &str, values: &str, attributes: &str) -> Mapping {
    let mapping = Mapping::new(jwks, PLAIN);
    std::fs::write(
        &mapping.path,
        format!(
            "issuer = \"{ISSUER}\"\naudience = \"{AUDIENCE}\"\njwks = \"{jwks}\"\n{PLAIN}\n\n\
             [actor]\nclaim = \"role\"\n\n[actor.values]\n{values}\n\n\
             [attributes.\"shelf.book.Librarian\"]\n{attributes}\n"
        ),
    )
    .expect("the shelf mapping is written");
    mapping
}

const BOTH: &str = "librarian = \"shelf.book.Librarian\"\nvisitor = \"shelf.book.Visitor\"";

fn shelf_claims(role: Value, extra: &Value) -> Value {
    let mut claims = json!({
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": now() + 3600,
        "role": role,
    });
    for (name, value) in extra.as_object().expect("an object") {
        claims[name] = value.clone();
    }
    claims
}

/// `operation` on the shelf run with `token`, `body` where given.
fn shelf_call(served: &Served, operation: &str, token: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = shelf_route(operation);
    let bearer = format!("Bearer {token}");
    exchange(
        served.address,
        &method,
        &path,
        &[("Authorization", &bearer)],
        body,
    )
}

#[test]
fn an_optional_attribute_left_unmapped_starts_and_its_token_binds() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = shelf_mapping(&jwks.url(), BOTH, "branch = \"branch\"\nlevel = \"level\"");
    let served = serve_on(SHELF, &["--caller-token", &mapping.path, "--observe"])
        .unwrap_or_else(|stopped| panic!("an unmapped optional attribute starts: {stopped:?}"));

    let librarian = signer.sign(&shelf_claims(
        json!("librarian"),
        &json!({"branch": "North", "level": 3, "nickname": 5}),
    ));
    let (status, body) = shelf_call(&served, "shelf.book.Catalogue", &librarian, None);
    assert_eq!(status, 200, "a librarian without a nickname binds: {body}");
    for path in [
        "/__spindle/requests",
        "/__spindle/metrics",
        "/__spindle/topology",
    ] {
        let (status, body) = get_as(&served, path, &librarian);
        assert_eq!(status, 200, "a binding caller reads {path}: {body}");
    }
    let (status, body) = shelf_call(
        &served,
        "shelf.book.ShelveBook",
        &librarian,
        Some(&json!({"book_id": A_VISIT, "branch": "North"})),
    );
    assert_ne!(status, 403, "the librarian is granted ShelveBook: {body}");

    let visitor = signer.sign(&shelf_claims(
        json!("visitor"),
        &json!({"branch": "Nowhere", "level": "high"}),
    ));
    let (status, body) = get_as(&served, "/__spindle/requests", &visitor);
    assert_eq!(
        status, 200,
        "a visitor binds whatever claims it carries beside its role: {body}"
    );
}

#[test]
fn a_librarian_token_whose_attributes_do_not_bind_is_no_caller() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let mapping = shelf_mapping(
        &jwks.url(),
        BOTH,
        "branch = \"branch\"\nlevel = \"level\"\nnickname = \"nick\"",
    );
    let served = serve_on(SHELF, &["--caller-token", &mapping.path, "--observe"])
        .unwrap_or_else(|stopped| panic!("the mapping starts: {stopped:?}"));
    let not_granted = json!({"refused": "not granted", "actor": null});

    let binds = signer.sign(&shelf_claims(
        json!("librarian"),
        &json!({"branch": "South", "level": 1, "nick": "Ada"}),
    ));
    assert_eq!(
        get_as(&served, "/__spindle/requests", &binds).0,
        200,
        "the binding librarian reads the log"
    );

    for (case, extra) in [
        (
            "a level as a string",
            json!({"branch": "North", "level": "3"}),
        ),
        (
            "a level as a fraction",
            json!({"branch": "North", "level": 3.5}),
        ),
        ("a level of null", json!({"branch": "North", "level": null})),
        (
            "a branch no variant names",
            json!({"branch": "East", "level": 3}),
        ),
        (
            "a branch in another case",
            json!({"branch": "north", "level": 3}),
        ),
        ("no branch", json!({"level": 3})),
        ("no level", json!({"branch": "North"})),
        (
            "an optional nickname of the wrong type",
            json!({"branch": "North", "level": 3, "nick": 5}),
        ),
    ] {
        let token = signer.sign(&shelf_claims(json!("librarian"), &extra));
        for path in [
            "/__spindle/requests",
            "/__spindle/metrics",
            "/__spindle/topology",
        ] {
            let (status, body) = get_as(&served, path, &token);
            assert_eq!(status, 403, "{case}: {path}: {body}");
            assert_eq!(body, not_granted, "{case}: {path}");
        }
        let (status, body) = shelf_call(&served, "shelf.book.Catalogue", &token, None);
        assert_eq!(status, 403, "{case}: the view: {body}");
        assert_eq!(body, not_granted, "{case}: the view");
    }

    for (case, role) in [
        (
            "a role list naming the librarian and the visitor",
            json!(["librarian", "visitor"]),
        ),
        ("a role list naming the visitor alone", json!(["visitor"])),
    ] {
        let token = signer.sign(&shelf_claims(role, &json!({})));
        let (status, body) = get_as(&served, "/__spindle/requests", &token);
        assert_eq!(status, 403, "{case}: {body}");
    }
    let librarian_unbound = signer.sign(&shelf_claims(json!("librarian"), &json!({})));
    let (status, _) = get_as(&served, "/__spindle/requests", &librarian_unbound);
    assert_eq!(
        status, 403,
        "a librarian that does not bind is not read as the visitor it would bind as"
    );

    let (_, log) = get_as(&served, "/__spindle/requests", &binds);
    assert_eq!(
        log["recorded"], 8,
        "only the eight views are recorded, nothing under /__spindle/: {log}"
    );
}

#[test]
fn a_required_attribute_mapped_from_no_claim_exits_1_and_an_unmapped_actor_does_not() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(Answering::Keys(vec![signer.jwk()]));
    let url = jwks.url();
    for (case, values, attributes, named) in [
        (
            "the level left out",
            BOTH,
            "branch = \"branch\"",
            ["shelf.book.Librarian", "level"],
        ),
        (
            "only the optional nickname mapped",
            BOTH,
            "nickname = \"nick\"",
            ["shelf.book.Librarian", "branch"],
        ),
        (
            "a second role mapped to the librarian, the level left out",
            "head = \"shelf.book.Librarian\"",
            "branch = \"branch\"",
            ["shelf.book.Librarian", "level"],
        ),
    ] {
        let mapping = shelf_mapping(&url, values, attributes);
        let Err(stopped) = serve_on(SHELF, &["--caller-token", &mapping.path]) else {
            panic!("{case}: the run listened");
        };
        assert_eq!(stopped.code, Some(1), "{case}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{case}: {stopped:?}");
        stopped.line_with(&["--caller-token", &mapping.path, named[0], named[1]]);
    }

    let only_visitor = Mapping::new(&url, PLAIN);
    std::fs::write(
        &only_visitor.path,
        format!(
            "issuer = \"{ISSUER}\"\naudience = \"{AUDIENCE}\"\njwks = \"{url}\"\n{PLAIN}\n\n\
             [actor]\nclaim = \"role\"\n\n[actor.values]\nvisitor = \"shelf.book.Visitor\"\n"
        ),
    )
    .expect("the mapping is written");
    let served = serve_on(SHELF, &["--caller-token", &only_visitor.path, "--observe"])
        .unwrap_or_else(|stopped| panic!("an actor no role maps needs no attributes: {stopped:?}"));
    let visitor = signer.sign(&shelf_claims(json!("visitor"), &json!({})));
    assert_eq!(get_as(&served, "/__spindle/requests", &visitor).0, 200);
}
