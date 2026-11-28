//! Adversary cases of wave 31 U3 (`story:listener-tls`) against `run --tls-cert --tls-key
//! --tls-client-ca`: one flag of a pair alone, certificate chains, key encodings, an encrypted key,
//! files of the wrong kind, client certificates the CA did not sign for a client or that expired,
//! TLS 1.2 and 1.3, the proxy's secret and the health routes over TLS, SIGTERM with TLS
//! connections open, and the flags on a run that serves no HTTP.
//!
//! Every certificate is generated here with `rcgen`; the clients are blocking rustls clients on
//! loopback. The fixture `tests/fixtures/trusted-caller` declares one component, `front`, reached
//! over the network.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "front";
const HEADER: &str = "X-Spindle-Caller";
const SECRET_HEADER: &str = "Spindle-Proxy-Secret";
const SECRET: &str = "adversary-proxy-secret-31";
const MEMBER: &str = "porch.visit.Member";
const RECORD: &str = "porch.visit.RecordVisit";
const PERSON: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
/// How long `run` may take to print its first line, or to exit.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

// ---------------------------------------------------------------------------------------------
// PEM and DER, written here so no dependency is added.

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(char::from(
                    ALPHABET[((n >> (18 - 6 * index)) & 63) as usize],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn pem(label: &str, der: &[u8]) -> String {
    let encoded = base64(der);
    let mut text = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        text.push_str(std::str::from_utf8(line).expect("base64 is ASCII"));
        text.push('\n');
    }
    text.push_str(&format!("-----END {label}-----\n"));
    text
}

/// The tag, content and rest of one DER element at the start of `input`.
fn element(input: &[u8]) -> (u8, &[u8], &[u8]) {
    let tag = input[0];
    let (length, header) = if input[1] < 0x80 {
        (usize::from(input[1]), 2)
    } else {
        let bytes = usize::from(input[1] & 0x7f);
        let length = input[2..2 + bytes]
            .iter()
            .fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
        (length, 2 + bytes)
    };
    (
        tag,
        &input[header..header + length],
        &input[header + length..],
    )
}

fn encoded(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    if content.len() < 0x80 {
        out.push(u8::try_from(content.len()).expect("a short length"));
    } else {
        let length = content.len().to_be_bytes();
        let start = length
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(length.len() - 1);
        out.push(0x80 | u8::try_from(length.len() - start).expect("a long length"));
        out.extend_from_slice(&length[start..]);
    }
    out.extend_from_slice(content);
    out
}

/// The SEC1 `ECPrivateKey` of a P-256 key given as PKCS #8, its curve named in `[0]` as an
/// `openssl ec` key carries it.
fn sec1_of(pkcs8: &[u8]) -> Vec<u8> {
    let (_, info, _) = element(pkcs8);
    let (_, _, rest) = element(info); // version
    let (_, _, rest) = element(rest); // algorithm
    let (tag, wrapped, _) = element(rest);
    assert_eq!(tag, 0x04, "the private key is an OCTET STRING");
    let (_, key, _) = element(wrapped);
    let (_, _, rest) = element(key); // version 1
    let (tag, private, mut rest) = element(rest);
    assert_eq!(tag, 0x04, "the EC private key is an OCTET STRING");
    let mut public = None;
    while !rest.is_empty() {
        let (tag, content, next) = element(rest);
        if tag == 0xa1 {
            public = Some(content.to_vec());
        }
        rest = next;
    }
    let p256: [u8; 10] = [0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
    let mut content = encoded(0x02, &[1]);
    content.extend(encoded(0x04, private));
    content.extend(encoded(0xa0, &p256));
    if let Some(public) = public {
        content.extend(encoded(0xa1, &public));
    }
    encoded(0x30, &content)
}

// ---------------------------------------------------------------------------------------------
// Certificates.

struct Authority {
    issuer: rcgen::CertifiedIssuer<'static, rcgen::KeyPair>,
}

struct Issued {
    certificate: CertificateDer<'static>,
    certificate_pem: String,
    key: rcgen::KeyPair,
}

impl Issued {
    fn key_der(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key.serialize_der().into())
    }
}

fn ca_params(name: &str) -> rcgen::CertificateParams {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, name);
    params
}

impl Authority {
    fn generate(name: &str) -> Self {
        let key = rcgen::KeyPair::generate().expect("a CA key");
        let issuer =
            rcgen::CertifiedIssuer::self_signed(ca_params(name), key).expect("the CA signs itself");
        Self { issuer }
    }

    /// An intermediate CA this one signs.
    fn intermediate(&self, name: &str) -> Self {
        let key = rcgen::KeyPair::generate().expect("an intermediate key");
        let issuer = rcgen::CertifiedIssuer::signed_by(ca_params(name), key, &self.issuer)
            .expect("the CA signs the intermediate");
        Self { issuer }
    }

    fn pem(&self) -> String {
        self.issuer.pem()
    }

    fn issue_with(
        &self,
        key: rcgen::KeyPair,
        names: Vec<String>,
        usage: rcgen::ExtendedKeyUsagePurpose,
        expired: bool,
    ) -> Issued {
        let mut params = rcgen::CertificateParams::new(names).expect("certificate parameters");
        params.extended_key_usages = vec![usage];
        if expired {
            params.not_before = rcgen::date_time_ymd(2001, 1, 1);
            params.not_after = rcgen::date_time_ymd(2002, 1, 1);
        }
        let certificate = params
            .signed_by(&key, &*self.issuer)
            .expect("the CA issues the certificate");
        Issued {
            certificate: certificate.der().clone(),
            certificate_pem: certificate.pem(),
            key,
        }
    }

    fn server_with(&self, key: rcgen::KeyPair) -> Issued {
        self.issue_with(
            key,
            vec!["127.0.0.1".to_owned()],
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
            false,
        )
    }

    fn server(&self) -> Issued {
        self.server_with(rcgen::KeyPair::generate().expect("a key"))
    }

    fn client(&self) -> Issued {
        self.client_for(rcgen::ExtendedKeyUsagePurpose::ClientAuth, false)
    }

    fn client_for(&self, usage: rcgen::ExtendedKeyUsagePurpose, expired: bool) -> Issued {
        self.issue_with(
            rcgen::KeyPair::generate().expect("a key"),
            vec!["proxy.test".to_owned()],
            usage,
            expired,
        )
    }

    fn roots(&self) -> rustls::RootCertStore {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(self.issuer.der().clone())
            .expect("the CA certificate is a root");
        roots
    }
}

/// A client trusting `authority` alone, on `versions`, presenting `chain` and its key where given.
fn client_on(
    authority: &Authority,
    versions: &[&'static rustls::SupportedProtocolVersion],
    chain: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> Arc<rustls::ClientConfig> {
    let builder = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(versions)
        .expect("the protocol versions")
        .with_root_certificates(authority.roots());
    Arc::new(match chain {
        Some((chain, key)) => builder
            .with_client_auth_cert(chain, key)
            .expect("the client certificate and key"),
        None => builder.with_no_client_auth(),
    })
}

fn client(authority: &Authority, presenting: Option<&Issued>) -> Arc<rustls::ClientConfig> {
    client_on(
        authority,
        rustls::DEFAULT_VERSIONS,
        presenting.map(|issued| (vec![issued.certificate.clone()], issued.key_der())),
    )
}

// ---------------------------------------------------------------------------------------------
// Files and the run.

struct Files {
    directory: tempfile::TempDir,
}

impl Files {
    fn new() -> Self {
        Self {
            directory: tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))
                .expect("a temporary directory"),
        }
    }

    fn write(&self, name: &str, text: &str) -> String {
        let path = self.directory.path().join(name);
        std::fs::write(&path, text).expect("the file is written");
        path.to_str().expect("a UTF-8 path").to_owned()
    }

    fn directory(&self, name: &str) -> String {
        let path = self.directory.path().join(name);
        std::fs::create_dir(&path).expect("the directory is made");
        path.to_str().expect("a UTF-8 path").to_owned()
    }
}

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

#[derive(Debug)]
struct Stopped {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `run` on `spec` and laptop with `arguments` added.
fn run_on(spec: &str, arguments: &[&str]) -> Result<Served, Stopped> {
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
        let listening = !line.is_empty();
        if !listening {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
        if listening {
            // Keep the pipe drained so the run never blocks on stdout.
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        }
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

fn run(arguments: &[&str]) -> Result<Served, Stopped> {
    run_on(SPEC, arguments)
}

fn serving(arguments: &[&str]) -> Served {
    run(arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

fn stopped(arguments: &[&str]) -> Stopped {
    match run(arguments) {
        Ok(served) => panic!("{arguments:?}: run listened on {}", served.address),
        Err(stopped) => stopped,
    }
}

type Tls = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

/// A TLS connection to `address` as `config` says, its handshake completed; the error that ended
/// the handshake otherwise.
fn connect(address: SocketAddr, config: Arc<rustls::ClientConfig>) -> std::io::Result<Tls> {
    let stream = TcpStream::connect_timeout(&address, ANSWER)?;
    stream.set_read_timeout(Some(ANSWER))?;
    stream.set_write_timeout(Some(ANSWER))?;
    let name = ServerName::IpAddress(Ipv4Addr::LOCALHOST.into());
    let connection = rustls::ClientConnection::new(config, name).map_err(std::io::Error::other)?;
    let mut tls = rustls::StreamOwned::new(connection, stream);
    while tls.conn.is_handshaking() {
        tls.conn.complete_io(&mut tls.sock)?;
    }
    Ok(tls)
}

fn send(
    stream: &mut impl Write,
    address: SocketAddr,
    line: &str,
    headers: &[(&str, &str)],
    body: &str,
    close: bool,
) -> std::io::Result<()> {
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let connection = if close { "Connection: close\r\n" } else { "" };
    write!(
        stream,
        "{line} HTTP/1.1\r\nHost: {address}\r\n{connection}{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// One answer read off `stream` by its `Content-Length`: its status and body.
fn answer(stream: &mut impl Read) -> std::io::Result<(u16, String)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                format!(
                    "the connection ended inside the head: {:?}",
                    String::from_utf8_lossy(&head)
                ),
            ));
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

/// One request over a TLS connection of its own: the status and body, or the error that ended it.
fn https(
    address: SocketAddr,
    config: Arc<rustls::ClientConfig>,
    line: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> std::io::Result<(u16, String)> {
    let mut tls = connect(address, config)?;
    send(&mut tls, address, line, headers, body, true)?;
    answer(&mut tls)
}

fn live(address: SocketAddr, config: Arc<rustls::ClientConfig>) -> std::io::Result<(u16, String)> {
    https(address, config, "GET /__spindle/live", &[], "")
}

/// The line of stderr holding `needle`.
fn line_with<'a>(stopped: &'a Stopped, needle: &str) -> &'a str {
    stopped
        .stderr
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line names {needle:?}: {stopped:?}"))
}

/// A certificate and key written as `run` reads them.
fn written(files: &Files, server: &Issued) -> (String, String) {
    (
        files.write("server.pem", &server.certificate_pem),
        files.write("server.key", &server.key.serialize_pem()),
    )
}

// ---------------------------------------------------------------------------------------------
// The flags.

/// One TLS flag without the flag it requires exits 2 before anything listens, naming the flag it
/// lacks (D6 A).
#[test]
fn one_tls_flag_without_its_pair_exits_2() {
    let authority = Authority::generate("adversary run CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let ca = files.write("clients.pem", &authority.pem());
    for (arguments, lacking) in [
        (vec!["--tls-cert", cert.as_str()], "--tls-key"),
        (vec!["--tls-key", key.as_str()], "--tls-cert"),
        (vec!["--tls-client-ca", ca.as_str()], "--tls-cert"),
        (
            vec!["--tls-client-ca", ca.as_str(), "--tls-cert", cert.as_str()],
            "--tls-key",
        ),
        (
            vec!["--tls-client-ca", ca.as_str(), "--tls-key", key.as_str()],
            "--tls-cert",
        ),
    ] {
        let stopped = stopped(&arguments);
        assert_eq!(stopped.code, Some(2), "{arguments:?}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{arguments:?}: {stopped:?}");
        assert!(
            stopped.stderr.contains(lacking),
            "{arguments:?} names {lacking}: {stopped:?}"
        );
    }
}

/// `--tls-cert` and `--tls-key` on a run that serves no HTTP are refused, naming the flag, as
/// `--address` is.
#[test]
fn tls_flags_on_a_run_serving_no_http_are_refused() {
    let authority = Authority::generate("adversary run CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let stopped = match run_on("ess", &["--tls-cert", &cert, "--tls-key", &key]) {
        Ok(served) => panic!("a run serving no HTTP listened on {}", served.address),
        Err(stopped) => stopped,
    };
    assert_ne!(stopped.code, Some(0), "{stopped:?}");
    line_with(&stopped, &format!("--tls-cert {cert} would be ignored"));
}

/// Off loopback TLS, even with a client CA, does not stand in for the trusted proxy: the run is
/// refused before listening (D5 A). A documentation address (RFC 5737) no host holds.
#[test]
fn tls_does_not_lift_the_trusted_proxy_off_loopback() {
    let authority = Authority::generate("adversary run CA");
    let clients = Authority::generate("adversary proxy CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let ca = files.write("clients.pem", &clients.pem());
    let stopped = stopped(&[
        "--address",
        "192.0.2.1",
        "--tls-cert",
        &cert,
        "--tls-key",
        &key,
        "--tls-client-ca",
        &ca,
    ]);
    assert_eq!(stopped.code, Some(2), "{stopped:?}");
    assert!(
        stopped.stderr.contains("--trusted-caller-header"),
        "{stopped:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The files.

/// A certificate file holding the chain, the server's certificate then its intermediate, is served
/// whole: a client trusting only the root verifies the listener.
#[test]
fn a_chained_certificate_file_is_served_whole() {
    let root = Authority::generate("adversary root CA");
    let intermediate = root.intermediate("adversary intermediate CA");
    let server = intermediate.server();
    let files = Files::new();
    let cert = files.write(
        "chain.pem",
        &format!("{}{}", server.certificate_pem, intermediate.pem()),
    );
    let key = files.write("server.key", &server.key.serialize_pem());
    let served = serving(&["--tls-cert", &cert, "--tls-key", &key]);
    let (status, body) =
        live(served.address, client(&root, None)).expect("HTTPS through the chain");
    assert_eq!(status, 200, "{body}");
}

/// The chain in the wrong order, the intermediate first, exits 2 naming both files: the first
/// certificate is not the key's.
#[test]
fn a_chain_with_the_intermediate_first_exits_2_naming_both_files() {
    let root = Authority::generate("adversary root CA");
    let intermediate = root.intermediate("adversary intermediate CA");
    let server = intermediate.server();
    let files = Files::new();
    let cert = files.write(
        "chain.pem",
        &format!("{}{}", intermediate.pem(), server.certificate_pem),
    );
    let key = files.write("server.key", &server.key.serialize_pem());
    let stopped = stopped(&["--tls-cert", &cert, "--tls-key", &key]);
    assert_eq!(stopped.code, Some(2), "{stopped:?}");
    let line = line_with(&stopped, "does not match");
    assert!(
        line.contains(&format!("--tls-key {key}")) && line.contains(&format!("--tls-cert {cert}")),
        "{line}"
    );
}

/// A key in SEC1 (`EC PRIVATE KEY`), PKCS #8 on P-384, and PKCS #8 on Ed25519 are each served.
#[test]
fn keys_in_sec1_and_other_curves_are_served() {
    let authority = Authority::generate("adversary run CA");
    let p256 = rcgen::KeyPair::generate().expect("a P-256 key");
    let sec1 = pem("EC PRIVATE KEY", &sec1_of(&p256.serialize_der()));
    let p256 = authority.server_with(p256);
    let p384 = authority.server_with(
        rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).expect("a P-384 key"),
    );
    let ed25519 = authority
        .server_with(rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).expect("an Ed25519 key"));
    for (name, server, key) in [
        ("sec1", &p256, sec1),
        ("p384", &p384, p384.key.serialize_pem()),
        ("ed25519", &ed25519, ed25519.key.serialize_pem()),
    ] {
        let files = Files::new();
        let cert = files.write("server.pem", &server.certificate_pem);
        let key = files.write("server.key", &key);
        let served = run(&["--tls-cert", &cert, "--tls-key", &key])
            .unwrap_or_else(|stopped| panic!("{name}: run stopped before listening: {stopped:?}"));
        let (status, body) = live(served.address, client(&authority, None))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(status, 200, "{name}: {body}");
    }
}

/// One file holding the certificate and the key, given to both flags, is served.
#[test]
fn one_file_holding_certificate_and_key_is_served() {
    let authority = Authority::generate("adversary run CA");
    let server = authority.server();
    let files = Files::new();
    let both = files.write(
        "server.pem",
        &format!("{}{}", server.certificate_pem, server.key.serialize_pem()),
    );
    let served = serving(&["--tls-cert", &both, "--tls-key", &both]);
    let (status, body) = live(served.address, client(&authority, None)).expect("HTTPS");
    assert_eq!(status, 200, "{body}");
}

/// An encrypted key, a directory for a file, and the certificate and key files swapped each exit 2
/// before listening, the line naming the flag and the file.
#[test]
fn files_of_the_wrong_kind_exit_2_naming_flag_and_file() {
    let authority = Authority::generate("adversary run CA");
    let server = authority.server();
    let files = Files::new();
    let (cert, key) = written(&files, &server);
    let encrypted = files.write(
        "encrypted.key",
        &pem("ENCRYPTED PRIVATE KEY", &server.key.serialize_der()),
    );
    let folder = files.directory("folder.pem");
    for (flag, arguments) in [
        (
            "--tls-key",
            vec!["--tls-cert", cert.as_str(), "--tls-key", encrypted.as_str()],
        ),
        (
            "--tls-cert",
            vec!["--tls-cert", folder.as_str(), "--tls-key", key.as_str()],
        ),
        (
            "--tls-key",
            vec!["--tls-cert", cert.as_str(), "--tls-key", folder.as_str()],
        ),
        (
            "--tls-client-ca",
            vec![
                "--tls-cert",
                cert.as_str(),
                "--tls-key",
                key.as_str(),
                "--tls-client-ca",
                folder.as_str(),
            ],
        ),
        (
            "--tls-cert",
            vec!["--tls-cert", key.as_str(), "--tls-key", cert.as_str()],
        ),
        (
            "--tls-client-ca",
            vec![
                "--tls-cert",
                cert.as_str(),
                "--tls-key",
                key.as_str(),
                "--tls-client-ca",
                key.as_str(),
            ],
        ),
    ] {
        let stopped = stopped(&arguments);
        assert_eq!(stopped.code, Some(2), "{arguments:?}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{arguments:?}: {stopped:?}");
        let file = arguments[arguments
            .iter()
            .position(|argument| *argument == flag)
            .expect("the flag is given")
            + 1];
        line_with(&stopped, &format!("{flag} {file}: "));
    }
}

// ---------------------------------------------------------------------------------------------
// Client certificates.

/// With `--tls-client-ca`: an expired client certificate the CA signed, and one the CA signed for
/// a server only, fail the handshake; a client presenting its intermediate is answered, and so is
/// one signed by the second CA of a CA file holding two.
#[test]
fn client_certificates_are_verified_as_a_client_ca_says() {
    let authority = Authority::generate("adversary run CA");
    let clients = Authority::generate("adversary proxy CA");
    let second = Authority::generate("adversary second proxy CA");
    let intermediate = clients.intermediate("adversary proxy intermediate CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let ca = files.write("clients.pem", &format!("{}{}", clients.pem(), second.pem()));
    let served = serving(&[
        "--tls-cert",
        &cert,
        "--tls-key",
        &key,
        "--tls-client-ca",
        &ca,
    ]);

    let expired = clients.client_for(rcgen::ExtendedKeyUsagePurpose::ClientAuth, true);
    let refused = live(served.address, client(&authority, Some(&expired)));
    assert!(
        refused.is_err(),
        "an expired client certificate is answered: {refused:?}"
    );

    let for_servers = clients.client_for(rcgen::ExtendedKeyUsagePurpose::ServerAuth, false);
    let refused = live(served.address, client(&authority, Some(&for_servers)));
    assert!(
        refused.is_err(),
        "a certificate for a server only is answered: {refused:?}"
    );

    let leaf = intermediate.client();
    let chained = client_on(
        &authority,
        rustls::DEFAULT_VERSIONS,
        Some((
            vec![leaf.certificate.clone(), intermediate.issuer.der().clone()],
            leaf.key_der(),
        )),
    );
    let (status, body) = live(served.address, chained).expect("a client with its intermediate");
    assert_eq!(status, 200, "{body}");

    let (status, body) = live(served.address, client(&authority, Some(&second.client())))
        .expect("the second CA's client");
    assert_eq!(status, 200, "{body}");

    let (status, body) = https(
        served.address,
        client(&authority, Some(&clients.client())),
        "GET /__spindle/ready",
        &[],
        "",
    )
    .expect("readiness with a client certificate");
    assert_eq!(status, 200, "{body}");
    let refused = https(
        served.address,
        client(&authority, None),
        "GET /__spindle/ready",
        &[],
        "",
    );
    assert!(
        refused.is_err(),
        "readiness without a client certificate is answered: {refused:?}"
    );
}

/// TLS 1.2 and TLS 1.3 are each served (D2 A).
#[test]
fn tls_1_2_and_1_3_are_both_served() {
    let authority = Authority::generate("adversary run CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let served = serving(&["--tls-cert", &cert, "--tls-key", &key]);
    for (name, version) in [
        ("1.2", &rustls::version::TLS12),
        ("1.3", &rustls::version::TLS13),
    ] {
        let config = client_on(&authority, &[version], None);
        let mut tls =
            connect(served.address, config).unwrap_or_else(|error| panic!("TLS {name}: {error}"));
        assert_eq!(
            tls.conn.protocol_version(),
            Some(version.version),
            "TLS {name} was negotiated"
        );
        send(
            &mut tls,
            served.address,
            "GET /__spindle/live",
            &[],
            "",
            true,
        )
        .expect("the request");
        let (status, body) = answer(&mut tls).expect("the answer");
        assert_eq!(status, 200, "TLS {name}: {body}");
    }
}

// ---------------------------------------------------------------------------------------------
// Served behaviour over TLS.

/// The trusted proxy's secret and caller header hold over TLS as over plain HTTP: a command without
/// the secret is refused `403`, one with it and a proof runs, and the console reads with the secret
/// alone.
#[test]
fn the_proxy_secret_and_console_hold_over_tls() {
    let authority = Authority::generate("adversary run CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let secret = files.write("proxy-secret", &format!("{SECRET}\n"));
    let served = serving(&[
        "--tls-cert",
        &cert,
        "--tls-key",
        &key,
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        &secret,
    ]);
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the trusted-caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == RECORD)
        .expect("a route for RecordVisit");
    let line = format!("{} {}", route.method.as_str().to_uppercase(), route.path);
    let proof = json!({"actor": MEMBER, "caller": {"person_id": PERSON}}).to_string();
    let body = json!({"visit_id": VISIT}).to_string();
    let config = client(&authority, None);

    let (status, refused) = https(
        served.address,
        Arc::clone(&config),
        &line,
        &[(HEADER, &proof)],
        &body,
    )
    .expect("the request without the secret");
    assert_eq!(status, 403, "{refused}");
    let (status, refused) = https(
        served.address,
        Arc::clone(&config),
        "GET /api/events",
        &[],
        "",
    )
    .expect("the console without the secret");
    assert_eq!(status, 403, "{refused}");

    let (status, recorded) = https(
        served.address,
        Arc::clone(&config),
        &line,
        &[(HEADER, &proof), (SECRET_HEADER, SECRET)],
        &body,
    )
    .expect("the proxy's request");
    assert_eq!(status, 202, "{recorded}");
    let (status, events) = https(
        served.address,
        config,
        "GET /api/events",
        &[(SECRET_HEADER, SECRET)],
        "",
    )
    .expect("the console with the secret");
    assert_eq!(status, 200, "{events}");
    let events: Value = serde_json::from_str(&events).expect("the events are JSON");
    assert_eq!(
        events["events"][0]["payload"]["person_id"], PERSON,
        "{events}"
    );
}

fn exited_within(child: &mut Child, within: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = child.try_wait().expect("the run is polled") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// SIGTERM with an idle kept-alive TLS connection, a client silent in its handshake and one that
/// sent half a hello: the run exits 143 within its bound, and the idle TLS client reads the end of
/// its connection.
#[test]
fn sigterm_with_tls_connections_open_exits_143() {
    let authority = Authority::generate("adversary run CA");
    let files = Files::new();
    let (cert, key) = written(&files, &authority.server());
    let mut served = serving(&["--tls-cert", &cert, "--tls-key", &key, "--drain", "30"]);
    let address = served.address;
    let mut idle = connect(address, client(&authority, None)).expect("the TLS connection");
    send(&mut idle, address, "GET /__spindle/live", &[], "", false).expect("the request");
    let (status, body) = answer(&mut idle).expect("the answer");
    assert_eq!(status, 200, "{body}");
    let _silent = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    let mut partial = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    partial
        .write_all(&[0x16, 0x03, 0x01, 0x02, 0x00])
        .expect("half a hello");
    std::thread::sleep(Duration::from_millis(300));
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("the signal is sent");
    let status = exited_within(&mut served.child, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143), "{status:?}");
    let mut rest = Vec::new();
    let ended = idle.read_to_end(&mut rest);
    assert!(
        !matches!(&ended, Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "the idle TLS connection is still open after the run exited: {ended:?}"
    );
}
