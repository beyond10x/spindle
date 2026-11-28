//! `run` serving HTTP with `--caller-token <FILE>` (`story:verified-caller-token`): each request
//! carries a signed token, `Authorization: Bearer <JWT>`, which the run verifies itself against
//! the issuer's key set (JWKS), its issuer, audience and expiry, and maps to the actor and caller
//! attributes of the model as the mapping file says.
//!
//! The fixture `tests/fixtures/trusted-caller` declares one component, `front`, reached over the
//! network: `Member` carries `person_id` and may record a visit, which records the caller's
//! `person_id`; `Auditor` carries no attribute and alone may read `VisitLog`.
//!
//! Every key is made here with `rcgen` (P-256), every token signed here with it, and the key set
//! served by a listener on loopback inside this test; no key is written to the tree.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustls::pki_types::ServerName;
use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "front";
const ISSUER: &str = "https://issuer.test";
const AUDIENCE: &str = "porch";
const MEMBER: &str = "porch.visit.Member";
const RECORD: &str = "porch.visit.RecordVisit";
const LOG: &str = "porch.visit.VisitLog";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
/// An IPv4-mapped IPv6 loopback address: off loopback by `spindle_ingress::exposed`, reached here
/// without leaving the host.
const MAPPED_LOOPBACK: &str = "::ffff:127.0.0.1";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(50);
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

/// `bytes` in unpadded base64url, as a JWK carries a coordinate.
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

/// A P-256 signing key, named `kid` in the key set.
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

    /// The public key as a JWK.
    fn jwk(&self) -> Value {
        let point = self.key.public_key_raw();
        assert_eq!(point.len(), 65, "an uncompressed P-256 point");
        json!({
            "kty": "EC",
            "crv": "P-256",
            "use": "sig",
            "alg": "ES256",
            "kid": self.kid,
            "x": base64url(&point[1..33]),
            "y": base64url(&point[33..]),
        })
    }

    /// `claims` signed with ES256 under this key's id.
    fn sign(&self, claims: &Value) -> String {
        self.sign_as(&self.kid, claims)
    }

    /// `claims` signed with ES256 by this key, its header naming the key `kid`.
    fn sign_as(&self, kid: &str, claims: &Value) -> String {
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(kid.to_owned());
        let key = jsonwebtoken::EncodingKey::from_ec_der(&self.key.serialize_der());
        jsonwebtoken::encode(&header, claims, &key).expect("the token is signed")
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs()
}

/// The claims of a token the mapping accepts: issuer, audience, an hour to run, the role
/// `role`, and `sub` where given.
fn claims(role: &str, sub: Option<&str>) -> Value {
    let mut claims = json!({
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": now() + 3600,
        "iat": now(),
        "role": role,
    });
    if let Some(sub) = sub {
        claims["sub"] = json!(sub);
    }
    claims
}

/// A key set served over plain HTTP on loopback, the keys it lists changeable while it serves.
struct Jwks {
    address: SocketAddr,
    keys: Arc<Mutex<Vec<Value>>>,
}

impl Jwks {
    fn serving(keys: Vec<Value>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("the key set binds");
        let address = listener.local_addr().expect("its address");
        let keys = Arc::new(Mutex::new(keys));
        let served = Arc::clone(&keys);
        std::thread::spawn(move || {
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
                let body = json!({"keys": *served.lock().expect("the keys")}).to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { address, keys }
    }

    fn url(&self) -> String {
        format!("http://{}/jwks", self.address)
    }

    fn set(&self, keys: Vec<Value>) {
        *self.keys.lock().expect("the keys") = keys;
    }
}

/// The mapping file of a token run whose key set is at `jwks`, refreshed every `refresh` seconds,
/// in a directory of its own.
struct Mapping {
    directory: tempfile::TempDir,
    path: String,
}

impl Mapping {
    fn new(jwks: &str, refresh: u64) -> Self {
        Self::with(jwks, refresh, "", "")
    }

    /// [`Mapping::new`], `values` added to `[actor.values]` and `attributes` to the member's
    /// `[attributes]` table.
    fn with(jwks: &str, refresh: u64, values: &str, attributes: &str) -> Self {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let path = directory.path().join("caller-token.toml");
        std::fs::write(
            &path,
            format!(
                r#"issuer = "{ISSUER}"
audience = "{AUDIENCE}"
jwks = "{jwks}"
algorithms = ["ES256"]
leeway_seconds = 0
refresh_seconds = {refresh}

[actor]
claim = "role"

[actor.values]
member = "porch.visit.Member"
auditor = "porch.visit.Auditor"
{values}

[attributes."porch.visit.Member"]
person_id = "sub"
{attributes}
"#
            ),
        )
        .expect("the mapping is written");
        let path = path.to_str().expect("a UTF-8 path").to_owned();
        Self { directory, path }
    }

    /// `text` written beside the mapping as `name`; its path.
    fn write(&self, name: &str, text: &str) -> String {
        let path = self.directory.path().join(name);
        std::fs::write(&path, text).expect("the file is written");
        path.to_str().expect("a UTF-8 path").to_owned()
    }
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

/// How a run that stopped without listening ended: its exit code, stdout and stderr.
#[derive(Debug)]
struct Stopped {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Stopped {
    /// The line of stderr holding every one of `needles`.
    fn line_with(&self, needles: &[&str]) -> &str {
        self.stderr
            .lines()
            .find(|line| needles.iter().all(|needle| line.contains(needle)))
            .unwrap_or_else(|| panic!("no line names all of {needles:?}: {self:?}"))
    }
}

/// `run` on the fixture and laptop with `arguments` added.
fn serve(arguments: &[&str]) -> Result<Served, Stopped> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(arguments)
        .env_remove("SPINDLE_SETTING_CONSOLE")
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
    Ok(Served { child, address })
}

/// `run --caller-token <mapping>` with `extra` arguments, listening.
fn token_run(mapping: &Mapping, extra: &[&str]) -> Served {
    let mut arguments = vec!["--caller-token", mapping.path.as_str()];
    arguments.extend_from_slice(extra);
    serve(&arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

/// One request's text: `method` `path`, `headers` as given, `body` as JSON.
fn request_text(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> String {
    let body = body.map(Value::to_string).unwrap_or_default();
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// The status, the head and the JSON body (`null` for none) of a whole answer.
fn parsed(answer: &[u8]) -> (u16, String, Value) {
    let answer = String::from_utf8_lossy(answer).into_owned();
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
    (status, head.to_owned(), body)
}

/// One HTTP/1.1 exchange on its own connection: the status, the head and the body.
fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, String, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .write_all(request_text(address, method, path, headers, body).as_bytes())
        .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    parsed(&answer)
}

/// One request for `operation` with `token` as its bearer token, where given.
fn call(
    served: &Served,
    operation: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> (u16, Value) {
    let (method, path) = route(operation);
    let (status, _, body) = get_with(served.address, &method, &path, token, body);
    (status, body)
}

fn get_with(
    address: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> (u16, String, Value) {
    let authorization = token.map(|token| format!("Bearer {token}"));
    let headers: Vec<(&str, &str)> = authorization
        .as_deref()
        .map(|value| ("Authorization", value))
        .into_iter()
        .collect();
    exchange(address, method, path, &headers, body)
}

fn not_proven() -> Value {
    json!({"refused": "not granted", "actor": null})
}

#[test]
fn a_request_with_a_valid_token_runs_as_its_mapped_caller() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let served = token_run(&mapping, &[]);

    let member = signer.sign(&claims("member", Some(PERSON)));
    let (status, recorded) = call(
        &served,
        RECORD,
        Some(&member),
        Some(&json!({"visit_id": VISIT})),
    );
    assert_eq!(status, 202, "{recorded}");
    assert_eq!(
        recorded["published"][0]["payload"]["person_id"], PERSON,
        "{{caller: person_id}} reads the attribute the token's sub maps to: {recorded}"
    );

    let (status, body) = call(&served, LOG, Some(&member), None);
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({"refused": "not granted", "actor": MEMBER}),
        "the grant check runs for the mapped actor"
    );
    let auditor = signer.sign(&claims("auditor", None));
    let (status, rows) = call(&served, LOG, Some(&auditor), None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(
        rows["rows"][0]["person_id"], PERSON,
        "the auditor reads the visit the member's token recorded: {rows}"
    );
}

#[test]
fn a_bad_token_is_refused_and_writes_nothing() {
    let signer = Signer::new("k1");
    let stranger = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let served = token_run(&mapping, &["--observe"]);

    let member = claims("member", Some(PERSON));
    let mut expired = member.clone();
    expired["exp"] = json!(now() - 3600);
    let mut issuer = member.clone();
    issuer["iss"] = json!("https://another-issuer.test");
    let mut audience = member.clone();
    audience["aud"] = json!("another-audience");
    let unsigned = format!(
        "{}.{}.",
        base64url(br#"{"alg":"none","typ":"JWT"}"#),
        base64url(member.to_string().as_bytes())
    );
    let cases: Vec<(&str, Option<String>)> = vec![
        ("expired", Some(signer.sign(&expired))),
        ("another issuer", Some(signer.sign(&issuer))),
        ("another audience", Some(signer.sign(&audience))),
        ("an unknown key id", Some(signer.sign_as("k9", &member))),
        (
            "a signature that does not verify",
            Some(stranger.sign(&member)),
        ),
        ("alg none", Some(unsigned)),
        ("no token", None),
    ];
    for (case, token) in &cases {
        let (method, path) = route(RECORD);
        let (status, head, body) = get_with(
            served.address,
            &method,
            &path,
            token.as_deref(),
            Some(&json!({"visit_id": VISIT})),
        );
        assert_eq!(status, 401, "{case}: {body}");
        assert!(
            head.to_ascii_lowercase()
                .contains("www-authenticate: bearer"),
            "{case}: a 401 names the scheme: {head}"
        );
        let (status, body) = call(&served, LOG, token.as_deref(), None);
        assert_eq!(status, 401, "{case}, a view: {body}");
    }

    let auditor = signer.sign(&claims("auditor", None));
    let (status, _, log) = get_with(
        served.address,
        "GET",
        "/__spindle/requests",
        Some(&auditor),
        None,
    );
    assert_eq!(status, 200, "{log}");
    assert_eq!(
        log["recorded"], 0,
        "no refused token is recorded or counted: {log}"
    );
    let (status, _, metrics) = get_with(
        served.address,
        "GET",
        "/__spindle/metrics",
        Some(&auditor),
        None,
    );
    assert_eq!(status, 200, "{metrics}");
    let metrics = metrics.as_str().unwrap_or_default().to_owned();
    assert!(
        !metrics
            .lines()
            .any(|line| line.starts_with("spindle_http_requests_total{")),
        "no request is counted: {metrics}"
    );
    let (status, rows) = call(&served, LOG, Some(&auditor), None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows, json!({"rows": []}), "no row is written");
}

#[test]
fn a_claim_that_maps_to_no_caller_of_the_model_is_refused() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let served = token_run(&mapping, &[]);

    let mut unnamed = claims("member", Some(PERSON));
    unnamed.as_object_mut().expect("an object").remove("role");
    for (case, claims) in [
        (
            "a role the mapping does not name",
            claims("stranger", Some(PERSON)),
        ),
        ("no role claim", unnamed),
        ("a required attribute left out", claims("member", None)),
        ("an attribute of the wrong type", {
            let mut claims = claims("member", None);
            claims["sub"] = json!(7);
            claims
        }),
    ] {
        let token = signer.sign(&claims);
        let (status, body) = call(
            &served,
            RECORD,
            Some(&token),
            Some(&json!({"visit_id": VISIT})),
        );
        assert_eq!(status, 403, "{case}: {body}");
        assert_eq!(body, not_proven(), "{case}");
    }
    let auditor = signer.sign(&claims("auditor", None));
    let (status, rows) = call(&served, LOG, Some(&auditor), None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows, json!({"rows": []}), "nothing is written");
}

/// Polls `GET VisitLog` with `token` until it answers `status`, for at most 15 s.
fn answers_within(served: &Served, token: &str, status: u16) -> bool {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if call(served, LOG, Some(token), None).0 == status {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn keys_rotate_without_a_restart() {
    let first = Signer::new("k1");
    let second = Signer::new("k2");
    let jwks = Jwks::serving(vec![first.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 1);
    let served = token_run(&mapping, &[]);
    let by_first = first.sign(&claims("auditor", None));
    let by_second = second.sign(&claims("auditor", None));

    assert_eq!(call(&served, LOG, Some(&by_first), None).0, 200);
    assert_eq!(
        call(&served, LOG, Some(&by_second), None).0,
        401,
        "a key the set does not list verifies nothing"
    );
    jwks.set(vec![first.jwk(), second.jwk()]);
    assert!(
        answers_within(&served, &by_second, 200),
        "a key added to the set verifies after the next refresh"
    );
    jwks.set(vec![second.jwk()]);
    assert!(
        answers_within(&served, &by_first, 401),
        "a key removed from the set no longer verifies"
    );
    assert_eq!(call(&served, LOG, Some(&by_second), None).0, 200);
}

#[test]
fn a_token_run_refuses_the_trusted_caller_header_beside_it() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let secret = mapping.write("proxy-secret", "s3cret-proxy-token-7f2a\n");
    let Err(stopped) = serve(&[
        "--caller-token",
        &mapping.path,
        "--trusted-caller-header",
        "X-Spindle-Caller",
        "--trusted-caller-secret",
        &secret,
    ]) else {
        panic!("run listened with a token and a trusted caller header");
    };
    assert_eq!(stopped.code, Some(1), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&["--caller-token", "--trusted-caller-header"]);
}

#[test]
fn a_token_run_off_loopback_exits_2() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let Err(stopped) = serve(&[
        "--caller-token",
        &mapping.path,
        "--address",
        MAPPED_LOOPBACK,
    ]) else {
        panic!("a token run listened off loopback without TLS");
    };
    assert_eq!(stopped.code, Some(2), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&["--caller-token", "--tls-cert", "--tls-key"]);

    let authority_key = rcgen::KeyPair::generate().expect("a CA key");
    let mut authority = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
    authority.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    authority.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let issuer =
        rcgen::CertifiedIssuer::self_signed(authority, authority_key).expect("the CA signs itself");
    let server_key = rcgen::KeyPair::generate().expect("a server key");
    let mut server =
        rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()]).expect("server parameters");
    server.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let certificate = server
        .signed_by(&server_key, &*issuer)
        .expect("the CA issues the certificate");
    let cert = mapping.write("server.pem", &certificate.pem());
    let key = mapping.write("server.key", &server_key.serialize_pem());

    let served = token_run(
        &mapping,
        &[
            "--address",
            MAPPED_LOOPBACK,
            "--tls-cert",
            &cert,
            "--tls-key",
            &key,
        ],
    );
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(issuer.der().clone())
        .expect("the CA certificate is a root");
    let config = Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth(),
    );
    let https = |token: Option<&str>| {
        let (method, path) = route(LOG);
        let authorization = token.map(|token| format!("Bearer {token}"));
        let headers: Vec<(&str, &str)> = authorization
            .as_deref()
            .map(|value| ("Authorization", value))
            .into_iter()
            .collect();
        let stream = TcpStream::connect_timeout(&served.address, ANSWER).expect("it accepts");
        stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
        let name = ServerName::IpAddress(Ipv4Addr::LOCALHOST.into());
        let connection =
            rustls::ClientConnection::new(Arc::clone(&config), name).expect("a TLS client");
        let mut tls = rustls::StreamOwned::new(connection, stream);
        tls.write_all(request_text(served.address, &method, &path, &headers, None).as_bytes())
            .expect("the request is written over TLS");
        let mut answer = Vec::new();
        let _ = tls.read_to_end(&mut answer);
        parsed(&answer)
    };
    let auditor = signer.sign(&claims("auditor", None));
    let (status, _, rows) = https(Some(&auditor));
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows, json!({"rows": []}));
    let (status, _, body) = https(None);
    assert_eq!(status, 401, "{body}");
}

#[test]
fn a_jwks_unreachable_at_start_exits_1() {
    let closed = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
    let address = closed.local_addr().expect("its address");
    drop(closed);
    let url = format!("http://{address}/jwks");
    let mapping = Mapping::new(&url, 300);
    let Err(stopped) = serve(&["--caller-token", &mapping.path]) else {
        panic!("run listened without its key set");
    };
    assert_eq!(stopped.code, Some(1), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&[&url]);
}

#[test]
fn the_console_routes_are_off_on_a_token_run() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let Err(stopped) = serve(&["--caller-token", &mapping.path, "--console", "on"]) else {
        panic!("a token run listened with the console on");
    };
    assert_eq!(stopped.code, Some(2), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&["--console on", "--caller-token"]);

    let served = token_run(&mapping, &["--observe"]);
    for open in ["/__spindle/live", "/__spindle/ready", "/__spindle/seed"] {
        let (status, _, body) = get_with(served.address, "GET", open, None, None);
        assert_eq!(status, 200, "{open} answers without a token: {body}");
    }
    for closed in [
        "/__spindle/requests",
        "/__spindle/metrics",
        "/__spindle/topology",
        "/__spindle/ui",
        "/__spindle/other",
        "/api/events",
        "/api/model",
        "/no/route",
    ] {
        let (status, _, body) = get_with(served.address, "GET", closed, None, None);
        assert_eq!(status, 401, "{closed} needs a token: {body}");
    }
    let auditor = signer.sign(&claims("auditor", None));
    let (status, _, body) = get_with(
        served.address,
        "GET",
        "/__spindle/requests",
        Some(&auditor),
        None,
    );
    assert_eq!(
        status, 200,
        "a verified token reads the run's own routes: {body}"
    );
    for console in ["/api/events", "/api/model", "/api/served"] {
        let (status, _, body) = get_with(served.address, "GET", console, Some(&auditor), None);
        assert_eq!(
            status, 404,
            "{console} is not served on a token run: {body}"
        );
    }
    let (status, _, body) = get_with(
        served.address,
        "POST",
        "/api/reset",
        Some(&auditor),
        Some(&json!({})),
    );
    assert_eq!(status, 404, "no reset on a token run: {body}");
}

/// F2 of the correction round: a token that verifies and maps to no caller of the mapping reads
/// none of the run's own routes but the probes; it is answered `403` "not granted" naming no
/// actor, and is neither recorded nor counted.
#[test]
fn a_token_mapping_to_no_caller_reads_none_of_the_runs_own_routes() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    let mapping = Mapping::new(&jwks.url(), 300);
    let served = token_run(&mapping, &["--observe"]);
    let stranger = signer.sign(&claims("stranger", Some(PERSON)));

    for path in [
        "/__spindle/requests",
        "/__spindle/metrics",
        "/__spindle/topology",
        "/__spindle/ui",
        "/__spindle/other",
    ] {
        let (status, _, body) = get_with(served.address, "GET", path, Some(&stranger), None);
        assert_eq!(status, 403, "{path}: {body}");
        assert_eq!(
            body,
            not_proven(),
            "{path}: nothing the route holds is answered"
        );
    }
    for probe in ["/__spindle/live", "/__spindle/ready", "/__spindle/seed"] {
        let (status, _, body) = get_with(served.address, "GET", probe, Some(&stranger), None);
        assert_eq!(status, 200, "{probe}: {body}");
    }
    let auditor = signer.sign(&claims("auditor", None));
    let (status, _, log) = get_with(
        served.address,
        "GET",
        "/__spindle/requests",
        Some(&auditor),
        None,
    );
    assert_eq!(
        status, 200,
        "a token mapped to a caller reads the log: {log}"
    );
    assert_eq!(log["recorded"], 0, "no refusal is recorded: {log}");
}

/// F3 of the correction round: a mapping naming an actor, or an attribute of an actor, the
/// specification does not declare exits 1 before listening, the line naming `--caller-token`,
/// the file and what it names.
#[test]
fn a_mapping_naming_what_the_model_does_not_declare_exits_1() {
    let signer = Signer::new("k1");
    let jwks = Jwks::serving(vec![signer.jwk()]);
    for (case, mapping, named) in [
        (
            "a role mapped to an undeclared actor",
            Mapping::with(&jwks.url(), 300, "ghost = \"porch.visit.Ghost\"", ""),
            "porch.visit.Ghost",
        ),
        (
            "an attribute the actor does not declare",
            Mapping::with(&jwks.url(), 300, "", "rank = \"rank\""),
            "rank",
        ),
    ] {
        let Err(stopped) = serve(&["--caller-token", &mapping.path]) else {
            panic!("{case}: run listened");
        };
        assert_eq!(stopped.code, Some(1), "{case}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{case}: {stopped:?}");
        stopped.line_with(&["--caller-token", &mapping.path, named]);
    }

    // G3 of correction round 2: a mapped actor whose required attribute the mapping maps from
    // no claim could never be a caller.
    let unmapped = Mapping::new(&jwks.url(), 300);
    let text = std::fs::read_to_string(&unmapped.path).expect("the mapping reads");
    std::fs::write(&unmapped.path, text.replace("person_id = \"sub\"\n", ""))
        .expect("the mapping is rewritten");
    let Err(stopped) = serve(&["--caller-token", &unmapped.path]) else {
        panic!("a required attribute mapped from no claim: run listened");
    };
    assert_eq!(stopped.code, Some(1), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    stopped.line_with(&[
        "--caller-token",
        &unmapped.path,
        "porch.visit.Member",
        "person_id",
    ]);
}
