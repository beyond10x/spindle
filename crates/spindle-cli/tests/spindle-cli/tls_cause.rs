//! A decision call leaves the host only over verified TLS (`story:decision-calls-tls`): an
//! `[http.<name>]` call to an `https://` URL is made over TLS, its certificate verified against the
//! platform's roots or the PEM file `ca_file` names; an `http://` URL off loopback is refused when
//! the environment is read, before any connection.
//!
//! The desk is a TLS server on 127.0.0.1 this test runs, its CA and certificate generated here. It
//! records every byte it reads off the socket, and every request it decrypts.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::Value;
use spindle_capability::{Environment, EnvironmentError};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/http-cause";
const ASK: &str = "probe.approval.Ask";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
const SECRET: &str = "SPINDLE_TLS_APPROVAL_TOKEN";
const TOKEN: &str = "tls-approval-token-for-tests";
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory")
}

/// A test CA and a certificate it issued for 127.0.0.1.
struct Authority {
    /// The CA's certificate, PEM.
    ca: String,
    certificate: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
}

impl Authority {
    fn generate() -> Self {
        let ca_key = rcgen::KeyPair::generate().expect("a CA key");
        let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
        ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        ca.distinguished_name
            .push(rcgen::DnType::CommonName, "spindle test CA");
        let ca = rcgen::CertifiedIssuer::self_signed(ca, ca_key).expect("the CA signs itself");
        let key = rcgen::KeyPair::generate().expect("a server key");
        let mut server =
            rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()]).expect("server parameters");
        server.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let certificate = server
            .signed_by(&key, &*ca)
            .expect("the CA issues the server certificate");
        Self {
            ca: ca.pem(),
            certificate: certificate.der().clone(),
            key: PrivatePkcs8KeyDer::from(key.serialize_der()),
        }
    }
}

/// One request the desk decrypted.
#[derive(Debug, Clone)]
struct Received {
    line: String,
    headers: Vec<(String, String)>,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }
}

/// A TCP stream that copies every byte read from it into `raw`.
struct Recording {
    stream: TcpStream,
    raw: Arc<Mutex<Vec<u8>>>,
}

impl Read for Recording {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.stream.read(buffer)?;
        self.raw
            .lock()
            .expect("the raw record")
            .extend_from_slice(&buffer[..read]);
        Ok(read)
    }
}

impl Write for Recording {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

/// A TLS server on 127.0.0.1 answering `{"holds": true}` to every request it decrypts.
struct Desk {
    address: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
    raw: Arc<Mutex<Vec<u8>>>,
}

impl Desk {
    fn start(authority: &Authority) -> Self {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![authority.certificate.clone()],
            PrivateKeyDer::Pkcs8(authority.key.clone_key()),
        )
        .expect("the server certificate and key");
        let config = Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
        let address = listener.local_addr().expect("the desk has an address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let (recorded, copied) = (Arc::clone(&received), Arc::clone(&raw));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let config = Arc::clone(&config);
                let recorded = Arc::clone(&recorded);
                let raw = Arc::clone(&copied);
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(ANSWER));
                    let Ok(connection) = rustls::ServerConnection::new(config) else {
                        return;
                    };
                    let tls = rustls::StreamOwned::new(connection, Recording { stream, raw });
                    answer(tls, &recorded);
                });
            }
        });
        Self {
            address,
            received,
            raw,
        }
    }

    fn url(&self) -> String {
        format!("https://{}/approvals", self.address)
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the record").clone()
    }

    fn raw(&self) -> Vec<u8> {
        self.raw.lock().expect("the raw record").clone()
    }
}

fn answer(
    tls: rustls::StreamOwned<rustls::ServerConnection, Recording>,
    recorded: &Mutex<Vec<Received>>,
) {
    let mut reader = BufReader::new(tls);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    recorded.lock().expect("the record").push(Received {
        line: line.trim_end().to_owned(),
        headers,
    });
    let mut tls = reader.into_inner();
    let body = r#"{"holds": true}"#;
    let _ = write!(
        tls,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

/// An environment document binding the cause to the call `name`, declared by `table`.
fn document(name: &str, table: &str) -> String {
    format!(
        "format = \"spindle-environment/1\"\nname = \"approvals\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"{CAPABILITY}\" = [\"{name}\"]\n\"external.*\" = [\"operator-prompt\"]\n\n\
         [http.{name}]\nmethod = \"POST\"\n{table}"
    )
}

fn write(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("approvals.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

/// `toml`'s basic string for `text`: a Windows path keeps its backslashes.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

fn ask(id: &str) -> String {
    format!(r#"{{"command": "{ASK}", "input": {{"request_id": "{id}", "amount": 7}}}}"#) + "\n"
}

fn run(environment: &str, lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", environment])
        .env(SECRET, TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the commands are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

#[test]
fn an_off_host_http_call_is_refused() {
    let refused = Environment::parse(&document(
        "x",
        "url = \"http://example.invalid/\"\ntimeout_ms = 1000\n",
    ));
    let Err(error) = refused else {
        panic!("an off-host http:// call is refused when the document is read: {refused:?}");
    };
    assert!(
        matches!(error, EnvironmentError::HttpOffHost { ref name } if name == "x"),
        "{error:?}"
    );
    assert!(error.to_string().contains("[http.x]"), "{error}");

    // 0.0.0.0 reaches a listener on 127.0.0.1 where the platform routes it there; a desk on that
    // port shows that `run` refuses before it opens any connection.
    let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
    listener.set_nonblocking(true).expect("a non-blocking desk");
    let port = listener.local_addr().expect("an address").port();
    for url in [
        "http://example.invalid/".to_owned(),
        format!("http://0.0.0.0:{port}/approvals"),
    ] {
        let directory = tempdir();
        let environment = write(
            &directory,
            &document(
                "x",
                &format!("url = \"{url}\"\ntimeout_ms = 1000\nsecret_env = \"{SECRET}\"\n"),
            ),
        );
        let output = run(&environment, &ask("r1"));
        assert_eq!(output.status.code(), Some(1), "{url}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("[http.x]") && stderr.contains("loopback"),
            "{url}: names the call and why: {stderr}"
        );
        assert!(output.stdout.is_empty(), "{url}: {output:?}");
    }
    assert!(
        matches!(listener.accept(), Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "no connection was opened"
    );
}

#[test]
fn a_loopback_http_call_is_accepted_and_any_other_host_is_not() {
    let parse = |url: &str| {
        Environment::parse(&document(
            "x",
            &format!("url = \"{url}\"\ntimeout_ms = 1000\n"),
        ))
    };
    for url in [
        "http://localhost:8080/approvals",
        "HTTP://LocalHost/",
        "http://127.0.0.1:1/",
        "http://127.255.255.254/",
        "http://[::1]:1/",
        "https://example.invalid/approvals",
        "https://203.0.113.7:8443/",
    ] {
        let parsed = parse(url);
        assert!(parsed.is_ok(), "{url}: {parsed:?}");
    }
    for url in [
        "http://example.invalid/",
        "http://0.0.0.0:1/",
        "http://10.0.0.1/",
        "http://128.0.0.1/",
        "http://[fe80::1]:1/",
        "http://[::ffff:127.0.0.1]/",
        "http://localhost.example/",
        "http://127.0.0.1.example/",
        "http://127.1/",
        "http:///approvals",
    ] {
        let parsed = parse(url);
        assert!(
            matches!(parsed, Err(EnvironmentError::HttpOffHost { ref name }) if name == "x"),
            "{url}: {parsed:?}"
        );
    }
}

#[test]
fn an_https_call_verifies_the_certificate() {
    let authority = Authority::generate();
    let desk = Desk::start(&authority);
    let directory = tempdir();
    let ca = directory.path().join("ca.pem");
    std::fs::write(&ca, &authority.ca).expect("the CA is written");
    let environment = write(
        &directory,
        &document(
            "approval-desk",
            &format!(
                "url = \"{}\"\ntimeout_ms = 5000\nsecret_env = \"{SECRET}\"\nca_file = {}\n",
                desk.url(),
                quoted(ca.to_str().expect("a UTF-8 path"))
            ),
        ),
    );

    let output = run(&environment, &ask("r1"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "approved", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["event"], "probe.approval.Approved",
        "the cause holds: {results:?}"
    );
    let received = desk.received();
    assert_eq!(received.len(), 1, "exactly one request: {received:?}");
    assert!(
        received[0].line.starts_with("POST /approvals "),
        "{received:?}"
    );
    assert_eq!(
        received[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str()),
        "the bearer header arrives: {received:?}"
    );
    assert_eq!(
        received[0].header("host"),
        Some(desk.address.to_string().as_str()),
        "{received:?}"
    );
    let raw = desk.raw();
    assert!(
        !raw.windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()),
        "the token crossed the socket only encrypted"
    );
}

#[test]
fn an_untrusted_certificate_fails_the_call() {
    let authority = Authority::generate();
    let desk = Desk::start(&authority);
    let directory = tempdir();
    let environment = write(
        &directory,
        &document(
            "approval-desk",
            &format!(
                "url = \"{}\"\ntimeout_ms = 5000\nsecret_env = \"{SECRET}\"\n",
                desk.url()
            ),
        ),
    );

    let output = run(&environment, &ask("r1"));
    assert_eq!(output.status.code(), Some(0), "the run goes on: {output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    assert!(
        results[0].get("events").is_none(),
        "no answer, nothing committed: {results:?}"
    );
    let message = results[0]["failure"]["message"]
        .as_str()
        .expect("a failure message");
    assert!(
        message.contains("approval-desk") && message.contains("certificate"),
        "names the call and the certificate: {message}"
    );
    assert!(desk.received().is_empty(), "no request was decrypted");
    let raw = desk.raw();
    assert!(!raw.is_empty(), "the client did reach the desk");
    assert!(
        !raw.windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes())
            && !raw.windows(6).any(|window| window == b"Bearer"),
        "the bearer token was never written to the stream"
    );
}

#[test]
fn a_ca_file_that_cannot_be_read_stops_the_run_before_any_request() {
    let authority = Authority::generate();
    let desk = Desk::start(&authority);
    let directory = tempdir();
    let missing = directory.path().join("missing.pem");
    let garbage = directory.path().join("garbage.pem");
    std::fs::write(&garbage, "no certificate here\n").expect("the file is written");
    for ca in [missing, garbage] {
        let environment = write(
            &directory,
            &document(
                "approval-desk",
                &format!(
                    "url = \"{}\"\ntimeout_ms = 5000\nca_file = {}\n",
                    desk.url(),
                    quoted(ca.to_str().expect("a UTF-8 path"))
                ),
            ),
        );
        let output = run(&environment, &ask("r1"));
        assert_eq!(output.status.code(), Some(1), "{ca:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("[http.approval-desk]") && stderr.contains("ca_file"),
            "{ca:?}: names the call and the key: {stderr}"
        );
    }
    assert!(desk.raw().is_empty(), "no connection was opened");
}

#[test]
fn a_ca_file_on_a_plain_http_call_is_refused() {
    let refused = Environment::parse(&document(
        "x",
        "url = \"http://127.0.0.1:1/\"\ntimeout_ms = 1000\nca_file = \"ca.pem\"\n",
    ));
    assert!(
        matches!(refused, Err(EnvironmentError::HttpCaWithoutTls { ref name }) if name == "x"),
        "{refused:?}"
    );
}
