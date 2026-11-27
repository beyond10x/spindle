//! A listener serving TLS (`story:listener-tls`, [`Listener::bind_reading_on_tls`]): with a
//! certificate and key it answers HTTPS and gives a plain HTTP request no HTTP answer; with a
//! client CA it answers only a client presenting a certificate that CA signed (mTLS).
//!
//! Every certificate is generated here with `rcgen`; the clients are blocking rustls clients on
//! 127.0.0.1. The fixture `tests/fixtures/grants` declares one component, `ledger`, and the
//! requests read `GET /__spindle/live`, which the listener answers on its own thread.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "ledger";
/// How long a client waits to connect, for the handshake and for an answer.
const ANSWER: Duration = Duration::from_secs(10);

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A CA and what it signs: a server certificate for 127.0.0.1 and client certificates.
struct Authority {
    issuer: rcgen::CertifiedIssuer<'static, rcgen::KeyPair>,
}

/// A certificate and its key.
struct Issued {
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

    fn certificate(&self) -> CertificateDer<'static> {
        self.issuer.der().clone()
    }

    fn issue(&self, names: Vec<String>, usage: rcgen::ExtendedKeyUsagePurpose) -> Issued {
        let key = rcgen::KeyPair::generate().expect("a key");
        let mut params = rcgen::CertificateParams::new(names).expect("certificate parameters");
        params.extended_key_usages = vec![usage];
        let certificate = params
            .signed_by(&key, &*self.issuer)
            .expect("the CA issues the certificate");
        Issued {
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
            .add(self.certificate())
            .expect("the CA certificate is a root");
        roots
    }
}

/// The server's configuration: `server`'s certificate and key, and with `clients` every client
/// required to present a certificate that CA signed.
fn server_config(server: &Issued, clients: Option<&Authority>) -> Arc<rustls::ServerConfig> {
    let builder = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions");
    let builder = match clients {
        Some(authority) => builder.with_client_cert_verifier(
            rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(authority.roots()),
                provider(),
            )
            .build()
            .expect("the client verifier"),
        ),
        None => builder.with_no_client_auth(),
    };
    let config = builder
        .with_single_cert(
            vec![server.certificate.clone()],
            PrivateKeyDer::Pkcs8(server.key.clone_key()),
        )
        .expect("the server certificate and key");
    Arc::new(config)
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

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"grants\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

struct Served {
    listener: Listener,
    _host: Host,
    _state: tempfile::TempDir,
}

impl Served {
    fn address(&self) -> SocketAddr {
        self.listener.local_addr()
    }
}

/// The ledger served on `127.0.0.1:<port>` over TLS as `config` says.
fn serve(config: Arc<rustls::ServerConfig>) -> Served {
    let ir = spindle_spec::load(&fixtures().join("grants"))
        .unwrap_or_else(|error| panic!("the grants fixture compiles: {error:?}"))
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let listener = Listener::bind_reading_on_tls(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        surface,
        host.reader(),
        config,
    )
    .expect("the TLS listener binds 127.0.0.1");
    Served {
        listener,
        _host: host,
        _state: state,
    }
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

/// The status of an HTTP answer, `None` where `answer` is no HTTP answer.
fn status(answer: &str) -> Option<u16> {
    answer
        .strip_prefix("HTTP/1.1 ")?
        .split(' ')
        .next()?
        .parse()
        .ok()
}

/// With a certificate and key the listener answers a request over TLS that a client verifies
/// against the issuing CA; a plain HTTP request on the same port gets no HTTP answer, and the
/// listener goes on answering TLS after it.
#[test]
fn a_tls_listener_answers_https_and_refuses_plain_http() {
    let authority = Authority::generate("spindle listener test CA");
    let served = serve(server_config(&authority.server(), None));
    let address = served.address();

    let answer = https(address, client_config(&authority, None)).expect("the HTTPS request");
    assert_eq!(status(&answer), Some(200), "{answer}");
    assert!(answer.contains("\"live\":true"), "{answer}");

    let mut plain = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    plain
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    plain
        .write_all(request(address).as_bytes())
        .expect("the plain request is written");
    let mut answer = Vec::new();
    // The listener ends the connection; whether it does so with a TLS alert or a reset, nothing
    // read is HTTP.
    let _ = plain.read_to_end(&mut answer);
    assert!(
        !answer.starts_with(b"HTTP/"),
        "a plain request got an HTTP answer: {:?}",
        String::from_utf8_lossy(&answer)
    );
    assert!(
        !answer
            .windows(b"\"live\"".len())
            .any(|window| window == b"\"live\""),
        "a plain request was answered: {answer:?}"
    );

    let other = Authority::generate("another CA");
    assert!(
        https(address, client_config(&other, None)).is_err(),
        "a client trusting another CA does not verify the listener"
    );

    let answer = https(address, client_config(&authority, None)).expect("HTTPS goes on");
    assert_eq!(status(&answer), Some(200), "{answer}");
}

/// With a client CA every client presents a certificate: one without, or with one another CA
/// signed, fails the handshake and gets no answer; one the CA signed is answered.
#[test]
fn a_client_certificate_is_required_when_a_ca_is_given() {
    let authority = Authority::generate("spindle listener test CA");
    let clients = Authority::generate("spindle proxy CA");
    let stranger = Authority::generate("a stranger's CA");
    let served = serve(server_config(&authority.server(), Some(&clients)));
    let address = served.address();

    let refused = https(address, client_config(&authority, None));
    assert!(
        refused.is_err(),
        "a client without a certificate is answered: {refused:?}"
    );

    let foreign = stranger.client();
    let refused = https(address, client_config(&authority, Some(&foreign)));
    assert!(
        refused.is_err(),
        "a client whose certificate another CA signed is answered: {refused:?}"
    );

    let proxy = clients.client();
    let answer =
        https(address, client_config(&authority, Some(&proxy))).expect("the proxy's request");
    assert_eq!(status(&answer), Some(200), "{answer}");
    assert!(answer.contains("\"live\":true"), "{answer}");
}
