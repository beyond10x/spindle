//! The bounds a listener serves within (`story:listener-limits`): a connection cap, a head size
//! and header count answered `431`, an idle keep-alive connection closed within the head read
//! time, and a request answered within its request time, `503` while no thread took it and `504`
//! once one did. Each case sets its bounds small through [`Limits`] and ends within 3 s.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`; the test answers each
//! command handed to it itself; over TLS it is served through a host of its own, on a certificate
//! a CA made for the test issued.

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Answer, Limits, Listener, Surface};

const COMPONENT: &str = "desk";
const OPEN: &str = "study.desk.OpenDesk";
/// A route the listener answers on its own thread, whatever the caller's thread does.
const LIVE: &str = "/__spindle/live";
/// How long a check waits for an answer that is due.
const DUE: Duration = Duration::from_secs(2);
/// How long a check waits to see that no answer comes.
const WITHHELD: Duration = Duration::from_millis(500);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The desk surface, recording requests, served within `limits`; and its `OpenDesk` route as
/// method and path.
fn surface(limits: Limits) -> (Surface, String, String) {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16)
        .limited(limits);
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == OPEN)
        .expect("the surface serves OpenDesk");
    let method = route.method.as_str().to_uppercase();
    let path = route.path.clone();
    (surface, method, path)
}

fn connect(address: SocketAddr) -> TcpStream {
    TcpStream::connect_timeout(&address, DUE).expect("the connection is made")
}

/// A keep-alive `GET target` with no body.
fn get(address: SocketAddr, target: &str) -> String {
    format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n")
}

/// A `Connection: close` request for `method path` with a JSON body.
fn command(address: SocketAddr, method: &str, path: &str) -> String {
    let body = json!({"desk_id": "d-1", "holder": "ada"}).to_string();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// One answer read off `stream` within `within`: its status and body, by its `Content-Length`;
/// `None` where none arrived, the connection closing or the time passing first.
fn answer(stream: &mut TcpStream, within: Duration) -> Option<(u16, String)> {
    stream
        .set_read_timeout(Some(within))
        .expect("a read timeout");
    let mut read = Vec::new();
    let mut chunk = [0u8; 4096];
    let head = loop {
        if let Some(end) = read.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(count) => read.extend_from_slice(&chunk[..count]),
        }
    };
    let text = String::from_utf8_lossy(&read[..head]).into_owned();
    let status = text.split(' ').nth(1)?.parse().ok()?;
    let length: usize = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    while read.len() < head + 4 + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => read.extend_from_slice(&chunk[..count]),
        }
    }
    let body = String::from_utf8_lossy(&read[head + 4..]).into_owned();
    Some((status, body))
}

/// Whether `stream` reads its end, any bytes before it allowed, within `within`.
fn closes(stream: &mut TcpStream, within: Duration) -> bool {
    let until = Instant::now() + within;
    let mut chunk = [0u8; 4096];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        stream.set_read_timeout(Some(left)).expect("a read timeout");
        match stream.read(&mut chunk) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return false;
            }
            Err(_) => return true,
        }
    }
}

/// Sends `request` on its own connection from a thread of its own: the answer read within
/// `within`, and how long it took from the request written.
fn sent(
    address: SocketAddr,
    request: String,
    within: Duration,
) -> JoinHandle<(Option<(u16, String)>, Duration)> {
    thread::spawn(move || {
        let mut stream = connect(address);
        stream
            .write_all(request.as_bytes())
            .expect("the request is written");
        let started = Instant::now();
        let answer = answer(&mut stream, within);
        (answer, started.elapsed())
    })
}

#[test]
fn a_connection_over_the_cap_waits_until_one_closes() {
    let limits = Limits {
        connections: 2,
        ..Limits::DEFAULT
    };
    let (surface, _, _) = surface(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut open = Vec::new();
    for place in 0..2 {
        let mut stream = connect(address);
        stream
            .write_all(get(address, LIVE).as_bytes())
            .expect("the request is written");
        assert!(
            answer(&mut stream, DUE).is_some(),
            "connection {place} under the cap is answered"
        );
        open.push(stream);
    }
    let mut third = connect(address);
    third
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert_eq!(
        answer(&mut third, WITHHELD),
        None,
        "a third connection is not answered while two stay open"
    );
    drop(open.remove(0));
    assert!(
        answer(&mut third, DUE).is_some(),
        "the third connection is answered once one of the two closed"
    );
}

#[test]
fn a_head_over_the_limit_answers_431() {
    let limits = Limits {
        head_bytes: 1024,
        headers: 8,
        ..Limits::DEFAULT
    };
    let (surface, method, path) = surface(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let long = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nX-Pad: {}\r\nContent-Length: 0\r\n\r\n",
        "a".repeat(2048)
    );
    let many = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{}Content-Length: 0\r\n\r\n",
        (0..12)
            .map(|index| format!("X-Extra-{index}: {index}\r\n"))
            .collect::<String>()
    );
    for (what, request) in [
        ("a head over 1024 bytes", long),
        ("a head of 14 headers", many),
    ] {
        let mut stream = connect(address);
        stream
            .write_all(request.as_bytes())
            .expect("the request is written");
        let answered = answer(&mut stream, DUE);
        assert_eq!(
            answered.as_ref().map(|(status, _)| *status),
            Some(431),
            "{what} answers 431: {answered:?}"
        );
        assert!(closes(&mut stream, DUE), "{what}: the connection closes");
    }
    assert!(
        listener.next(Duration::from_millis(100)).is_none(),
        "no request over the limits reaches the caller's thread"
    );
    let mut stream = connect(address);
    stream
        .write_all(get(address, "/__spindle/requests").as_bytes())
        .expect("the request is written");
    let (status, log) = answer(&mut stream, DUE).expect("the request log answers");
    assert_eq!(status, 200, "{log}");
    let log: Value = serde_json::from_str(&log).unwrap_or_else(|error| panic!("{error}: {log}"));
    assert_eq!(
        log["requests"],
        json!([]),
        "no request over the limits is recorded"
    );
}

/// A query of 60 KiB, as a consumer's long view list sends, is under the default head limit of
/// 64 KiB: the read reaches the caller's thread and its answer is written, not `431`.
#[test]
fn a_head_under_the_default_is_answered() {
    let (surface, _, _) = surface(Limits::DEFAULT);
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.AllDesks")
        .expect("the surface serves the view")
        .path
        .clone();
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let query = format!("pad={}", "q".repeat(60 * 1024));
    let request =
        format!("GET {view}?{query} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    let asked = sent(address, request, DUE);
    let exchange = listener
        .next(DUE)
        .expect("a head of 60 KiB reaches the caller's thread");
    assert_eq!(
        exchange.query().len(),
        query.len(),
        "the whole query is handed on"
    );
    exchange.respond(Answer {
        status: 200,
        body: Some(json!({"rows": []})),
        headers: Vec::new(),
    });
    let (answered, _) = asked.join().expect("the client's thread ends");
    let (status, body) = answered.expect("the read is answered");
    assert_eq!(status, 200, "a head under the default is answered: {body}");
}

#[test]
fn an_idle_connection_is_closed_and_frees_its_place() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, _, _) = surface(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut idle = connect(address);
    idle.write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert!(
        answer(&mut idle, DUE).is_some(),
        "the first connection is answered"
    );
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert!(
        closes(&mut idle, DUE),
        "a keep-alive connection that sends nothing more is closed within the head read time"
    );
    assert!(
        answer(&mut waiting, DUE).is_some(),
        "the connection waiting at the cap is answered once the idle one closed"
    );
}

#[test]
fn a_request_past_its_time_answers_and_frees_the_listener() {
    let limits = Limits {
        request_time: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = surface(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let held = sent(address, command(address, &method, &path), DUE);
    let exchange = listener.next(DUE).expect("the command is handed over");
    let (answered, took) = held.join().expect("the client's thread ends");
    let (status, body) = answered.expect("the held command is answered");
    assert_eq!(status, 504, "{body}");
    assert!(
        took >= Duration::from_millis(900) && took < DUE,
        "answered at its request time, not before: {took:?}"
    );
    exchange.respond(Answer::refusal(409, "answered too late"));

    let next = sent(address, command(address, &method, &path), DUE);
    let exchange = listener.next(DUE).expect("the next command is handed over");
    exchange.respond(Answer {
        status: 201,
        body: Some(json!({"opened": "d-1"})),
        headers: Vec::new(),
    });
    let (answered, _) = next.join().expect("the client's thread ends");
    let (status, body) = answered.expect("the next command is answered");
    assert_eq!(status, 201, "the next request is answered: {body}");
}

#[test]
fn a_queued_request_past_its_time_answers_503_and_is_never_handed() {
    let limits = Limits {
        request_time: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = surface(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let queued = sent(address, command(address, &method, &path), DUE);
    let (answered, took) = queued.join().expect("the client's thread ends");
    let (status, body) = answered.expect("the queued command is answered");
    assert_eq!(status, 503, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"));
    assert!(body["refused"].is_string(), "{body}");
    assert_eq!(body["committed"], json!(false), "{body}");
    assert!(
        took >= Duration::from_millis(900),
        "answered at its request time, not before: {took:?}"
    );
    assert!(
        listener.next(Duration::from_millis(200)).is_none(),
        "a request answered 503 past its time is never handed"
    );
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A CA made for the test, and a certificate for 127.0.0.1 it issued with its key.
fn certificates() -> (
    rustls::RootCertStore,
    CertificateDer<'static>,
    PrivatePkcs8KeyDer<'static>,
) {
    let key = rcgen::KeyPair::generate().expect("a CA key");
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    let issuer = rcgen::CertifiedIssuer::self_signed(params, key).expect("the CA signs itself");
    let server_key = rcgen::KeyPair::generate().expect("a key");
    let mut params = rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()])
        .expect("certificate parameters");
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let certificate = params
        .signed_by(&server_key, &*issuer)
        .expect("the CA issues the certificate");
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(issuer.der().clone())
        .expect("the CA certificate is a root");
    (
        roots,
        certificate.der().clone(),
        PrivatePkcs8KeyDer::from(server_key.serialize_der()),
    )
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"limits\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// A TLS connection to `address` whose handshake has completed, its socket's reads and writes
/// given up after `wait`.
fn tls_connect(
    address: SocketAddr,
    client: &Arc<rustls::ClientConfig>,
    wait: Duration,
) -> rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    let stream = connect(address);
    stream.set_read_timeout(Some(wait)).expect("a read timeout");
    stream
        .set_write_timeout(Some(wait))
        .expect("a write timeout");
    let connection = rustls::ClientConnection::new(
        Arc::clone(client),
        ServerName::IpAddress(address.ip().into()),
    )
    .expect("a client connection");
    let mut tls = rustls::StreamOwned::new(connection, stream);
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .expect("the handshake completes");
    }
    tls
}

/// Over TLS, as over plain TCP, a client that pipelines requests and never reads an answer holds
/// its place of the cap for the head read time at most once its answers stop being written: with
/// a cap of 1 and a head read time of 1 s, a TLS client behind it is answered within 3 s
/// (`story:listener-limits`, adversary finding F1).
#[test]
fn a_tls_client_that_never_reads_its_answers_does_not_hold_the_cap() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .limited(Limits {
            connections: 1,
            head_read: Duration::from_secs(1),
            ..Limits::DEFAULT
        });
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
    let (roots, certificate, key) = certificates();
    let server = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("the default protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![certificate], PrivateKeyDer::Pkcs8(key))
        .expect("the server certificate and key");
    let client = Arc::new(
        rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("the default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let listener = Listener::bind_reading_on_tls(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        0,
        surface,
        host.reader(),
        Arc::new(server),
    )
    .expect("the TLS listener binds");
    let address = listener.local_addr();

    let mut stuck = tls_connect(address, &client, Duration::from_millis(500));
    let batch = get(address, LIVE).repeat(256);
    let started = Instant::now();
    let mut sent = 0usize;
    while stuck
        .write_all(batch.as_bytes())
        .and_then(|()| stuck.flush())
        .is_ok()
    {
        sent += batch.len();
        assert!(
            started.elapsed() < Duration::from_secs(20) && sent < 256 << 20,
            "the listener kept reading: {sent} bytes in {:?}",
            started.elapsed()
        );
    }

    let mut waiting = tls_connect(address, &client, Duration::from_secs(3));
    write!(
        waiting,
        "GET {LIVE} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    waiting.flush().expect("the request is flushed");
    let mut read = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(count) = waiting.read(&mut chunk) {
        if count == 0 {
            break;
        }
        read.extend_from_slice(&chunk[..count]);
    }
    let answered = String::from_utf8_lossy(&read);
    assert!(
        answered.starts_with("HTTP/1.1 200"),
        "a TLS client that never reads ({sent} bytes of requests sent) holds the only place of \
         the cap: the client behind it read {answered:?}"
    );
    drop(stuck);
}
