//! Adversary cases of wave 31 U3 (`story:listener-tls`) against [`Listener::bind_reading_on_tls`]:
//! a plain HTTP request on the TLS port, a client that opens TCP and never finishes the handshake,
//! the drain with a TLS connection open, the health routes and the proxy's secret over TLS, and a
//! surface without the secret off loopback.
//!
//! Every certificate is generated here with `rcgen`; the clients are blocking rustls clients on
//! loopback. The fixture `tests/fixtures/grants` declares one component, `ledger`.

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Answer, Console, HEADER_READ, HTTP_LOOPBACK, Listener, Surface};

const COMPONENT: &str = "ledger";
const HEADER: &str = "x-vault-caller";
const SECRET: &str = "proxy-secret";
/// How long a client waits to connect, for the handshake and for an answer.
const ANSWER: Duration = Duration::from_secs(10);
/// What "at once" means here: far below [`HEADER_READ`], far above a loopback round trip.
const PROMPT: Duration = Duration::from_secs(3);
/// A grace far longer than [`HEADER_READ`], so a close that waits for a stalled handshake shows.
const LONG_GRACE: Duration = Duration::from_secs(30);

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

struct Authority {
    issuer: rcgen::CertifiedIssuer<'static, rcgen::KeyPair>,
}

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

    fn server(&self) -> Issued {
        let key = rcgen::KeyPair::generate().expect("a key");
        let mut params =
            rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned(), "::1".to_owned()])
                .expect("certificate parameters");
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let certificate = params
            .signed_by(&key, &*self.issuer)
            .expect("the CA issues the certificate");
        Issued {
            certificate: certificate.der().clone(),
            key: PrivatePkcs8KeyDer::from(key.serialize_der()),
        }
    }

    fn roots(&self) -> rustls::RootCertStore {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(self.issuer.der().clone())
            .expect("the CA certificate is a root");
        roots
    }
}

fn server_config(server: &Issued) -> Arc<rustls::ServerConfig> {
    let config = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![server.certificate.clone()],
            PrivateKeyDer::Pkcs8(server.key.clone_key()),
        )
        .expect("the server certificate and key");
    Arc::new(config)
}

fn client_config(authority: &Authority) -> Arc<rustls::ClientConfig> {
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("the default protocol versions")
            .with_root_certificates(authority.roots())
            .with_no_client_auth(),
    )
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

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixtures().join("grants"))
        .unwrap_or_else(|error| panic!("the grants fixture compiles: {error:?}"))
        .ir
}

/// The ledger's plain surface with the console routes.
fn console_surface(ir: &ess_compiler::EssIr) -> Surface {
    Surface::new(ir, &[COMPONENT.to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
}

struct Served {
    listener: Listener,
    authority: Authority,
    _host: Host,
    _state: tempfile::TempDir,
}

/// The ledger served over TLS on `ip`, `surface` made from its IR.
fn serve_on(ip: IpAddr, surface: impl FnOnce(&ess_compiler::EssIr) -> Surface) -> Served {
    let ir = ir();
    let surface = surface(&ir);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let authority = Authority::generate("adversary listener CA");
    let listener = Listener::bind_reading_on_tls(
        ip,
        0,
        surface,
        host.reader(),
        server_config(&authority.server()),
    )
    .expect("the TLS listener binds");
    Served {
        listener,
        authority,
        _host: host,
        _state: state,
    }
}

fn serve() -> Served {
    serve_on(IpAddr::V4(Ipv4Addr::LOCALHOST), console_surface)
}

type Tls = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

/// A TLS connection to `address`, its handshake completed.
fn connect(address: SocketAddr, config: Arc<rustls::ClientConfig>) -> Tls {
    let stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let connection =
        rustls::ClientConnection::new(config, ServerName::IpAddress(address.ip().into()))
            .expect("a client connection");
    let mut tls = rustls::StreamOwned::new(connection, stream);
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .expect("the handshake completes");
    }
    tls
}

/// One request on `stream`, `close` saying whether it asks for `Connection: close`.
fn send(
    stream: &mut impl Write,
    address: SocketAddr,
    line: &str,
    headers: &[(&str, &str)],
    body: &str,
    close: bool,
) {
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let connection = if close { "Connection: close\r\n" } else { "" };
    write!(
        stream,
        "{line} HTTP/1.1\r\nHost: {address}\r\n{connection}{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    stream.flush().expect("the request is flushed");
}

/// One answer read off `stream` by its `Content-Length`: its status and body.
fn answer(stream: &mut impl Read) -> (u16, String) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let read = stream.read(&mut byte).expect("the answer's head is read");
        assert_eq!(
            read,
            1,
            "the connection ended inside the head: {:?}",
            String::from_utf8_lossy(&head)
        );
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
    stream
        .read_exact(&mut body)
        .expect("the answer's body is read");
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// `GET path` with `headers` over a TLS connection of its own: the status and body.
fn https(served: &Served, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let address = served.listener.local_addr();
    let mut tls = connect(address, client_config(&served.authority));
    send(&mut tls, address, &format!("GET {path}"), headers, "", true);
    answer(&mut tls)
}

/// A TCP connection to `address` that sends `bytes` and nothing more, accepted by the listener
/// before this returns.
fn stalled(address: SocketAddr, bytes: &[u8]) -> TcpStream {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.write_all(bytes).expect("the bytes are written");
    stream
}

/// The start of a TLS record carrying a handshake message, announcing 512 bytes that never come.
const PARTIAL_HELLO: [u8; 5] = [0x16, 0x03, 0x01, 0x02, 0x00];

/// A plain HTTP request on the TLS port is not left to wait for [`HEADER_READ`]: the listener ends
/// the connection at once, with no HTTP answer.
#[test]
fn a_plain_request_on_the_tls_port_is_closed_at_once() {
    let served = serve();
    let address = served.listener.local_addr();
    let mut plain = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    plain
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let started = Instant::now();
    send(&mut plain, address, "GET /__spindle/live", &[], "", true);
    let mut read = Vec::new();
    let ended = plain.read_to_end(&mut read);
    let took = started.elapsed();
    assert!(
        !matches!(&ended, Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "the connection was not ended within {ANSWER:?}: {ended:?}"
    );
    assert!(took < PROMPT, "a plain request was ended after {took:?}");
    assert!(
        !read.starts_with(b"HTTP/"),
        "{:?}",
        String::from_utf8_lossy(&read)
    );
}

/// Clients that open TCP and never finish the handshake hold no slot of the accept loop: a TLS
/// client arriving after 32 of them is answered at once.
#[test]
fn stalled_handshakes_do_not_hold_the_accept_loop() {
    let served = serve();
    let address = served.listener.local_addr();
    let _held: Vec<TcpStream> = (0..32)
        .map(|index| stalled(address, if index % 2 == 0 { &[] } else { &PARTIAL_HELLO }))
        .collect();
    let started = Instant::now();
    let (status, body) = https(&served, "/__spindle/live", &[]);
    let took = started.elapsed();
    assert_eq!(status, 200, "{body}");
    assert!(
        took < PROMPT,
        "the TLS client behind 32 stalled ones waited {took:?}"
    );
}

/// A client that opens TCP and sends nothing is closed once [`HEADER_READ`] has passed, not later
/// and not never.
#[test]
fn a_stalled_handshake_is_closed_at_the_header_read_bound() {
    let served = serve();
    let address = served.listener.local_addr();
    let mut silent = stalled(address, &PARTIAL_HELLO);
    silent
        .set_read_timeout(Some(HEADER_READ * 2))
        .expect("a read timeout");
    let started = Instant::now();
    let mut read = Vec::new();
    let ended = silent.read_to_end(&mut read);
    let took = started.elapsed();
    assert!(
        !matches!(&ended, Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "a stalled handshake was still open after {took:?}"
    );
    assert!(
        took >= HEADER_READ - Duration::from_secs(1) && took < HEADER_READ + PROMPT,
        "a stalled handshake was closed after {took:?}, not at {HEADER_READ:?}"
    );
}

/// A close with a long grace does not wait for clients stalled in the handshake: they hold no
/// answer, so the listener closes them and returns at once.
#[test]
fn stalled_handshakes_do_not_hold_the_drain() {
    let Served {
        listener,
        authority: _authority,
        _host,
        _state,
    } = serve();
    let address = listener.local_addr();
    let mut silent = stalled(address, &[]);
    let mut partial = stalled(address, &PARTIAL_HELLO);
    // Each is accepted and its handshake under way before the close.
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    listener.close(LONG_GRACE);
    let took = started.elapsed();
    assert!(
        took < PROMPT,
        "the close waited {took:?} for stalled handshakes"
    );
    for stream in [&mut silent, &mut partial] {
        stream
            .set_read_timeout(Some(PROMPT))
            .expect("a read timeout");
        let mut read = Vec::new();
        let ended = stream.read_to_end(&mut read);
        assert!(
            !matches!(&ended, Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
            "a stalled handshake is still open after the close: {ended:?}"
        );
    }
}

/// An idle kept-alive TLS connection does not hold the drain, and its client reads the end of the
/// connection.
#[test]
fn an_idle_tls_connection_does_not_hold_the_drain() {
    let Served {
        listener,
        authority,
        _host,
        _state,
    } = serve();
    let address = listener.local_addr();
    let mut tls = connect(address, client_config(&authority));
    send(&mut tls, address, "GET /__spindle/live", &[], "", false);
    let (status, body) = answer(&mut tls);
    assert_eq!(status, 200, "{body}");
    let started = Instant::now();
    listener.close(LONG_GRACE);
    let took = started.elapsed();
    assert!(
        took < PROMPT,
        "the close waited {took:?} for an idle TLS connection"
    );
    let mut rest = Vec::new();
    let ended = tls.read_to_end(&mut rest);
    assert!(
        !matches!(&ended, Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "the idle TLS connection is still open after the close: {ended:?}"
    );
    assert!(rest.is_empty(), "{:?}", String::from_utf8_lossy(&rest));
}

/// Over TLS as over plain HTTP: an answer handed back before the close is written, and a request
/// still queued at the close reads `503`.
#[test]
fn a_close_writes_every_answer_over_tls() {
    let Served {
        listener,
        authority,
        _host,
        _state,
    } = serve();
    let address = listener.local_addr();
    let config = client_config(&authority);
    let reset = |config: Arc<rustls::ClientConfig>| {
        std::thread::spawn(move || {
            let mut tls = connect(address, config);
            send(
                &mut tls,
                address,
                "POST /api/reset",
                &[],
                "{\"rows\": 7}",
                true,
            );
            answer(&mut tls)
        })
    };
    let handed = reset(Arc::clone(&config));
    let exchange = listener.next(ANSWER).expect("the reset is handed over");
    assert_eq!(exchange.console(), Some(Console::Reset));
    let queued = reset(config);
    std::thread::sleep(Duration::from_millis(500));
    exchange.respond(Answer::refusal(400, "the world is refused"));
    listener.close(Duration::from_secs(5));
    let (status, body) = handed.join().expect("the client's thread ends");
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("the world is refused"), "{body}");
    let (status, body) = queued.join().expect("the client's thread ends");
    assert_eq!(status, 503, "a request queued at the close: {body}");
}

/// `GET /__spindle/live` and `/__spindle/ready` answer over TLS, and once the listener is stopping
/// readiness reads `503` over TLS too.
#[test]
fn the_health_routes_answer_over_tls() {
    let served = serve();
    let (status, body) = https(&served, "/__spindle/live", &[]);
    assert_eq!(status, 200, "{body}");
    let (status, body) = https(&served, "/__spindle/ready", &[]);
    assert_eq!(status, 200, "{body}");
    served.listener.stopping().store(true, Ordering::SeqCst);
    let (status, body) = https(&served, "/__spindle/ready", &[]);
    assert_eq!(status, 503, "{body}");
}

/// A surface requiring the proxy's secret refuses a TLS request without it `403`, and answers one
/// with it.
#[test]
fn the_proxy_secret_is_required_over_tls() {
    let served = serve_on(IpAddr::V6(Ipv6Addr::LOCALHOST), |ir| {
        Surface::proving_callers(ir, &[COMPONENT.to_owned()], None, HEADER, SECRET)
            .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
            .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
            .requiring_secret()
    });
    let (status, body) = https(&served, "/__spindle/live", &[]);
    assert_eq!(status, 403, "{body}");
    let (status, body) = https(&served, "/api/model", &[]);
    assert_eq!(status, 403, "{body}");
    let (status, body) = https(
        &served,
        "/__spindle/live",
        &[(Surface::SECRET_HEADER, SECRET)],
    );
    assert_eq!(status, 200, "{body}");
}

/// Off loopback a TLS listener, like a plain one, binds no surface that does not require the
/// proxy's secret: a documentation address (RFC 5737), refused before any bind.
#[test]
fn a_surface_without_a_secret_is_not_bound_off_loopback_over_tls() {
    let ir = ir();
    let open = console_surface(&ir);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
    let authority = Authority::generate("adversary listener CA");
    let exposed = Listener::bind_reading_on_tls(
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        0,
        open,
        host.reader(),
        server_config(&authority.server()),
    );
    assert_eq!(
        exposed.map(|_| ()).map_err(|error| error.kind()),
        Err(ErrorKind::InvalidInput),
        "a surface without the proxy's secret is not bound off loopback"
    );
}
