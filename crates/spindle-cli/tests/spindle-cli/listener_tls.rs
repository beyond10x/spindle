//! `run` serving HTTPS (`story:listener-tls`): `--tls-cert` with `--tls-key` serves TLS on the
//! listener, and `--tls-client-ca` requires every client to present a certificate that CA signed.
//! A certificate, key or CA file that cannot be read, and a key that does not match the
//! certificate, exit 2 before the listening line, with a line naming the flag and the file.
//!
//! Every certificate is generated here with `rcgen`, written into a temporary directory; the
//! clients are blocking rustls clients on 127.0.0.1. The fixture `tests/fixtures/trusted-caller`
//! declares one component, `front`, reached over the network; the requests read
//! `GET /__spindle/live`, which every served run answers.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use serde_json::Value;

const SPEC: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
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

/// A CA and what it signs.
struct Authority {
    issuer: rcgen::CertifiedIssuer<'static, rcgen::KeyPair>,
}

/// A certificate and its key, PEM and DER.
struct Issued {
    certificate_pem: String,
    key_pem: String,
    certificate: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
}

impl Authority {
    fn generate(name: &str) -> Self {
        let key = rcgen::KeyPair::generate().expect("a CA key");
        let mut params =
            rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::CrlSign,
        ];
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, name);
        let issuer = rcgen::CertifiedIssuer::self_signed(params, key).expect("the CA signs itself");
        Self { issuer }
    }

    fn pem(&self) -> String {
        self.issuer.pem()
    }

    fn issue(&self, names: Vec<String>, usage: rcgen::ExtendedKeyUsagePurpose) -> Issued {
        let key = rcgen::KeyPair::generate().expect("a key");
        let mut params = rcgen::CertificateParams::new(names).expect("certificate parameters");
        params.extended_key_usages = vec![usage];
        let certificate = params
            .signed_by(&key, &*self.issuer)
            .expect("the CA issues the certificate");
        Issued {
            certificate_pem: certificate.pem(),
            key_pem: key.serialize_pem(),
            certificate: certificate.der().clone(),
            key: PrivatePkcs8KeyDer::from(key.serialize_der()),
        }
    }

    fn server(&self) -> Issued {
        self.issue(
            vec!["127.0.0.1".to_owned()],
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        )
    }

    fn client(&self) -> Issued {
        self.issue(
            vec!["proxy.test".to_owned()],
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
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

/// The files a run reads: the server's certificate and key, and the client CA.
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

    /// `text` written to the file `name`, and its path.
    fn write(&self, name: &str, text: &str) -> String {
        let path = self.directory.path().join(name);
        std::fs::write(&path, text).expect("the file is written");
        path.to_str().expect("a UTF-8 path").to_owned()
    }

    /// The path of `name`, which is never written.
    fn absent(&self, name: &str) -> String {
        let path = self.directory.path().join(name);
        path.to_str().expect("a UTF-8 path").to_owned()
    }
}

/// A client trusting `authority` alone, presenting `certificate` where given.
fn client_config(authority: &Authority, certificate: Option<&Issued>) -> Arc<rustls::ClientConfig> {
    let builder = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_root_certificates(authority.roots());
    let config = match certificate {
        Some(issued) => builder
            .with_client_auth_cert(
                vec![issued.certificate.clone()],
                PrivateKeyDer::Pkcs8(issued.key.clone_key()),
            )
            .expect("the client certificate and key"),
        None => builder.with_no_client_auth(),
    };
    Arc::new(config)
}

/// A running `b10x-spindle run` serving HTTPS; stopped when dropped.
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

fn request(address: SocketAddr) -> String {
    format!("GET /__spindle/live HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
}

/// `GET /__spindle/live` over TLS as `config` says: the whole answer, or the error that ended the
/// handshake or the read.
fn https(address: SocketAddr, config: Arc<rustls::ClientConfig>) -> std::io::Result<String> {
    let stream = TcpStream::connect_timeout(&address, ANSWER)?;
    stream.set_read_timeout(Some(ANSWER))?;
    stream.set_write_timeout(Some(ANSWER))?;
    let name = ServerName::IpAddress(Ipv4Addr::LOCALHOST.into());
    let connection = rustls::ClientConnection::new(config, name).map_err(std::io::Error::other)?;
    let mut tls = rustls::StreamOwned::new(connection, stream);
    tls.write_all(request(address).as_bytes())?;
    tls.flush()?;
    let mut answer = Vec::new();
    tls.read_to_end(&mut answer)?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// `GET /__spindle/live` in plain HTTP: whatever arrived before the listener ended the
/// connection.
fn plain(address: SocketAddr) -> Vec<u8> {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .write_all(request(address).as_bytes())
        .expect("the plain request is written");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    answer
}

/// Whether `answer` is HTTP's `200`.
fn ok(answer: &str) -> bool {
    answer.starts_with("HTTP/1.1 200 ")
}

/// The error line of a run that stopped: the line of stderr holding `needle`.
fn line_with<'a>(stopped: &'a Stopped, needle: &str) -> &'a str {
    stopped
        .stderr
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line names {needle:?}: {stopped:?}"))
}

/// A file that cannot be read for each flag, a file holding no certificate, and a key that does
/// not match the certificate each exit 2 before the listening line, the line on stderr naming the
/// flag and the file, and for the mismatch both files.
#[test]
fn a_bad_certificate_file_exits_before_listening() {
    let authority = Authority::generate("spindle run test CA");
    let server = authority.server();
    let stranger = authority.server();
    let files = Files::new();
    let cert = files.write("server.pem", &server.certificate_pem);
    let key = files.write("server.key", &server.key_pem);
    let other_key = files.write("other.key", &stranger.key_pem);
    let missing = files.absent("missing.pem");
    let empty = files.write("empty.pem", "no certificate here\n");

    for (flag, arguments) in [
        (
            "--tls-cert",
            vec!["--tls-cert", &missing, "--tls-key", &key],
        ),
        ("--tls-cert", vec!["--tls-cert", &empty, "--tls-key", &key]),
        (
            "--tls-key",
            vec!["--tls-cert", &cert, "--tls-key", &missing],
        ),
        ("--tls-key", vec!["--tls-cert", &cert, "--tls-key", &empty]),
        (
            "--tls-client-ca",
            vec![
                "--tls-cert",
                &cert,
                "--tls-key",
                &key,
                "--tls-client-ca",
                &missing,
            ],
        ),
        (
            "--tls-client-ca",
            vec![
                "--tls-cert",
                &cert,
                "--tls-key",
                &key,
                "--tls-client-ca",
                &empty,
            ],
        ),
    ] {
        let Err(stopped) = serve(&arguments) else {
            panic!("{arguments:?}: run listened");
        };
        assert_eq!(stopped.code, Some(2), "{arguments:?}: {stopped:?}");
        assert!(stopped.stdout.is_empty(), "{arguments:?}: {stopped:?}");
        let file = arguments[arguments
            .iter()
            .position(|argument| *argument == flag)
            .expect("the flag is given")
            + 1];
        line_with(&stopped, &format!("{flag} {file}: "));
    }

    let Err(stopped) = serve(&["--tls-cert", &cert, "--tls-key", &other_key]) else {
        panic!("a key that does not match the certificate listened");
    };
    assert_eq!(stopped.code, Some(2), "{stopped:?}");
    assert!(stopped.stdout.is_empty(), "{stopped:?}");
    let line = line_with(&stopped, "does not match");
    assert!(
        line.contains(&format!("--tls-key {other_key}"))
            && line.contains(&format!("--tls-cert {cert}")),
        "the line names both files: {line}"
    );
}

/// `run` given `--tls-cert` and `--tls-key` answers HTTPS end to end, its listening line as
/// without them, and gives a plain HTTP request no HTTP answer.
#[test]
fn a_served_run_given_a_certificate_answers_https() {
    let authority = Authority::generate("spindle run test CA");
    let server = authority.server();
    let files = Files::new();
    let cert = files.write("server.pem", &server.certificate_pem);
    let key = files.write("server.key", &server.key_pem);

    let served = serve(&["--tls-cert", &cert, "--tls-key", &key])
        .unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"));
    let answer = https(served.address, client_config(&authority, None)).expect("HTTPS");
    assert!(ok(&answer), "{answer}");
    assert!(answer.contains("\"live\":true"), "{answer}");
    let answer = plain(served.address);
    assert!(
        !answer.starts_with(b"HTTP/"),
        "a plain request got an HTTP answer: {:?}",
        String::from_utf8_lossy(&answer)
    );
}

/// `run` given `--tls-client-ca` as well answers a client presenting a certificate that CA signed,
/// and no client without one.
#[test]
fn a_served_run_given_a_client_ca_requires_a_client_certificate() {
    let authority = Authority::generate("spindle run test CA");
    let clients = Authority::generate("spindle proxy CA");
    let server = authority.server();
    let files = Files::new();
    let cert = files.write("server.pem", &server.certificate_pem);
    let key = files.write("server.key", &server.key_pem);
    let ca = files.write("clients.pem", &clients.pem());

    let served = serve(&[
        "--tls-cert",
        &cert,
        "--tls-key",
        &key,
        "--tls-client-ca",
        &ca,
    ])
    .unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"));
    let refused = https(served.address, client_config(&authority, None));
    assert!(
        refused.is_err(),
        "a client without a certificate is answered: {refused:?}"
    );
    let foreign = authority.client();
    let refused = https(served.address, client_config(&authority, Some(&foreign)));
    assert!(
        refused.is_err(),
        "a client whose certificate another CA signed is answered: {refused:?}"
    );
    let proxy = clients.client();
    let answer =
        https(served.address, client_config(&authority, Some(&proxy))).expect("the proxy's HTTPS");
    assert!(ok(&answer), "{answer}");
}
