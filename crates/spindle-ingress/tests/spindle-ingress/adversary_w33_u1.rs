//! Adversary cases of wave 33 U1 (`story:listener-limits`) against the connection cap, the head
//! read time and the request time of [`Limits`]: a client that never reads its answers, a head
//! sent byte by byte, a TLS handshake that stalls at the cap, and requests whose time runs out
//! while the caller's thread takes them.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, answered by the test itself;
//! `tests/fixtures/grants` declares `ledger`, served over TLS through a host's reader.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Answer, Limits, Listener, Surface};

const COMPONENT: &str = "desk";
const OPEN: &str = "study.desk.OpenDesk";
const LIVE: &str = "/__spindle/live";
/// How long a check waits for an answer that is due.
const DUE: Duration = Duration::from_secs(3);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The desk surface served within `limits`, and its `OpenDesk` route as method and path.
fn desk(limits: Limits) -> (Surface, String, String) {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
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

fn get(address: SocketAddr, target: &str) -> String {
    format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n")
}

fn command(address: SocketAddr, method: &str, path: &str, desk_id: &str) -> String {
    let body = json!({"desk_id": desk_id, "holder": "ada"}).to_string();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// One answer read off `stream` within `within`: status and body; `None` where none arrived.
fn answer(
    stream: &mut impl Read,
    within: Option<&TcpStream>,
    wait: Duration,
) -> Option<(u16, String)> {
    if let Some(socket) = within {
        socket.set_read_timeout(Some(wait)).expect("a read timeout");
    }
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

fn plain_answer(stream: &mut TcpStream, wait: Duration) -> Option<(u16, String)> {
    let socket = stream.try_clone().expect("the socket clones");
    answer(stream, Some(&socket), wait)
}

/// A client that pipelines requests and never reads an answer is not idle: hyper stops reading
/// its heads once its answers fill the buffers, so the head read time never starts, and nothing
/// else bounds the connection. With a cap of 1 it holds the only place for as long as it stays
/// open; a second client is never answered. The story's bound ("idle connections cannot hold the
/// cap") is meant to free the place within the head read time or the request time (1 s each
/// here); this waits 3 s.
#[test]
fn a_client_that_never_reads_its_answers_does_not_hold_the_cap() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        request_time: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, _, _) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut stuck = connect(address);
    stuck
        .set_write_timeout(Some(Duration::from_millis(500)))
        .expect("a write timeout");
    let batch = get(address, LIVE).repeat(256);
    let started = Instant::now();
    let mut sent = 0usize;
    while let Ok(count) = stuck.write(batch.as_bytes()) {
        sent += count;
        assert!(
            started.elapsed() < Duration::from_secs(20) && sent < 256 << 20,
            "the listener kept reading: {sent} bytes in {:?}",
            started.elapsed()
        );
    }
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    let answered = plain_answer(&mut waiting, DUE);
    assert!(
        answered.is_some(),
        "a client that never reads ({sent} bytes of requests sent, none of the answers read) \
         holds the only place of the cap past the head read time and the request time: \
         a second client got no answer within {DUE:?}"
    );
    drop(stuck);
}

/// A head sent one byte at a time is closed at the head read time, counted from its first byte,
/// not reset by each byte: with a cap of 1 a second client is answered within 2.5 s.
#[test]
fn a_head_sent_byte_by_byte_frees_its_place_at_the_head_read_time() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, _, _) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut slow = connect(address);
    let trickle = thread::spawn(move || {
        let head = format!("GET {LIVE} HTTP/1.1\r\nHost: {address}\r\nX-Slow: ");
        let mut bytes = head.into_bytes().into_iter().chain(std::iter::repeat(b'a'));
        let until = Instant::now() + Duration::from_secs(4);
        while Instant::now() < until {
            let byte = bytes.next().expect("an endless head");
            if slow.write_all(&[byte]).is_err() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    });
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    let answered = plain_answer(&mut waiting, DUE);
    let took = started.elapsed();
    assert_eq!(answered.map(|(status, _)| status), Some(200));
    assert!(
        took < Duration::from_millis(2500),
        "the client behind a byte-by-byte head waited {took:?}"
    );
    assert!(
        trickle.join().expect("the trickle ends"),
        "the byte-by-byte connection was closed"
    );
}

/// No command is both answered `503` with `committed: false` and handed to the caller's thread:
/// with a request time of 300 ms and a thread that takes one command every 40 ms, some are taken
/// near their deadline and some lapse. Each one handed answers `201` or `504`; each one not handed
/// answers `503` with `committed: false`; and the two sets make up every command sent.
#[test]
fn no_command_answered_503_is_ever_handed() {
    const SENT: usize = 24;
    let limits = Limits {
        request_time: Duration::from_millis(300),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let clients: Vec<_> = (0..SENT)
        .map(|index| {
            let request = command(address, &method, &path, &format!("d-{index}"));
            thread::spawn(move || {
                let mut stream = connect(address);
                stream
                    .write_all(request.as_bytes())
                    .expect("the request is written");
                (format!("d-{index}"), plain_answer(&mut stream, DUE))
            })
        })
        .collect();
    let mut handed = BTreeSet::new();
    let until = Instant::now() + Duration::from_millis(2000);
    while Instant::now() < until {
        if let Some(exchange) = listener.next(Duration::from_millis(20)) {
            let body: Value = serde_json::from_slice(exchange.body()).expect("a JSON body");
            let desk_id = body["desk_id"].as_str().expect("a desk id").to_owned();
            assert!(handed.insert(desk_id.clone()), "{desk_id} handed twice");
            thread::sleep(Duration::from_millis(40));
            exchange.respond(Answer {
                status: 201,
                body: Some(json!({"opened": desk_id})),
                headers: Vec::new(),
            });
        }
    }
    let answers: BTreeMap<String, Option<(u16, String)>> = clients
        .into_iter()
        .map(|client| client.join().expect("the client's thread ends"))
        .collect();
    assert!(
        listener.next(Duration::from_millis(200)).is_none(),
        "nothing is handed after every client was answered"
    );
    let mut refused = 0;
    for (desk_id, answered) in &answers {
        let (status, body) = answered
            .clone()
            .unwrap_or_else(|| panic!("{desk_id} is answered"));
        if handed.contains(desk_id) {
            assert!(
                status == 201 || status == 504,
                "{desk_id} was handed and answered {status}: {body}"
            );
        } else {
            refused += 1;
            assert_eq!(status, 503, "{desk_id} was never handed: {body}");
            let body: Value = serde_json::from_str(&body).expect("a JSON refusal");
            assert_eq!(body["committed"], json!(false), "{desk_id}: {body}");
        }
    }
    assert_eq!(
        handed.len() + refused,
        SENT,
        "every command is accounted for"
    );
    assert!(
        refused > 0 && !handed.is_empty(),
        "the case takes some and lets some lapse: {} handed, {refused} refused",
        handed.len()
    );
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A self-signed CA and a server certificate for 127.0.0.1 it issued.
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

/// Over TLS, a client that opens TCP and stalls inside its handshake holds its place of the cap
/// for the head read time at most: with a cap of 1 and a head read time of 1 s, a TLS client
/// behind it completes its handshake and is answered within 2.5 s.
#[test]
fn a_stalled_tls_handshake_frees_its_place_at_the_head_read_time() {
    let ir = spindle_spec::load(&fixtures().join("grants"))
        .unwrap_or_else(|error| panic!("the grants fixture compiles: {error:?}"))
        .ir;
    let surface = Surface::new(&ir, &["ledger".to_owned()], None)
        .expect("the ledger surface is served")
        .limited(Limits {
            connections: 1,
            head_read: Duration::from_secs(1),
            ..Limits::DEFAULT
        });
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the ledger: {error}"));
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
    let mut stalled = connect(address);
    stalled
        .write_all(&[0x16, 0x03, 0x01, 0x02, 0x00])
        .expect("a partial hello is written");
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    let stream = connect(address);
    stream.set_read_timeout(Some(DUE)).expect("a read timeout");
    stream
        .set_write_timeout(Some(DUE))
        .expect("a write timeout");
    let connection =
        rustls::ClientConnection::new(client, ServerName::IpAddress(address.ip().into()))
            .expect("a client connection");
    let socket = stream.try_clone().expect("the socket clones");
    let mut tls = rustls::StreamOwned::new(connection, stream);
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .expect("the handshake completes");
    }
    write!(
        tls,
        "GET {LIVE} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    tls.flush().expect("the request is flushed");
    let answered = answer(&mut tls, Some(&socket), DUE);
    let took = started.elapsed();
    assert_eq!(answered.map(|(status, _)| status), Some(200));
    assert!(
        took < Duration::from_millis(2500),
        "the TLS client behind a stalled handshake waited {took:?}"
    );
    let mut rest = [0u8; 16];
    stalled
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("a read timeout");
    assert!(
        !matches!(stalled.read(&mut rest), Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)),
        "the stalled handshake was closed"
    );
}

// Round 2: against the fix that closes a connection whose answer cannot be written (`Stalling`),
// and what round 1 left: a client that trickles its reads, an honest reader of a large answer, a
// command whose client left while it was queued, a connection waiting at the cap when the
// listener closes, and a body that stalls.

/// A keep-alive `method path` with a JSON body for the desk `desk_id`.
fn kept_command(address: SocketAddr, method: &str, path: &str, desk_id: &str) -> String {
    let body = json!({"desk_id": desk_id, "holder": "ada"}).to_string();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Writes pipelined `GET /__spindle/live` requests on `stuck` until the listener stops reading
/// them, its answers unread; the bytes sent.
fn fill(stuck: &mut TcpStream, address: SocketAddr) -> usize {
    stuck
        .set_write_timeout(Some(Duration::from_millis(500)))
        .expect("a write timeout");
    let batch = get(address, LIVE).repeat(256);
    let started = Instant::now();
    let mut sent = 0usize;
    while let Ok(count) = stuck.write(batch.as_bytes()) {
        sent += count;
        assert!(
            started.elapsed() < Duration::from_secs(20) && sent < 256 << 20,
            "the listener kept reading: {sent} bytes in {:?}",
            started.elapsed()
        );
    }
    sent
}

/// Progress resets the write bound, but a read of one byte now and then is no progress: the
/// kernel opens no window for it, so the listener's write stays blocked and the connection is
/// closed at the head read time. With a cap of 1, a client that fills its answers and then reads
/// one byte every 250 ms does not keep a second client from being answered.
#[test]
fn a_client_reading_one_byte_at_a_time_does_not_hold_the_cap() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        request_time: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, _, _) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut stuck = connect(address);
    let sent = fill(&mut stuck, address);
    let trickle = thread::spawn(move || {
        stuck
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("a read timeout");
        let until = Instant::now() + Duration::from_secs(4);
        let mut byte = [0u8; 1];
        while Instant::now() < until {
            match stuck.read(&mut byte) {
                Ok(1) => thread::sleep(Duration::from_millis(250)),
                Ok(_) => return true,
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return true,
            }
        }
        false
    });
    let started = Instant::now();
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    let answered = plain_answer(&mut waiting, DUE);
    assert_eq!(
        answered.map(|(status, _)| status),
        Some(200),
        "a client trickling one byte at a time ({sent} bytes of requests sent) held the cap for \
         {:?}",
        started.elapsed()
    );
    drop(trickle);
}

/// The write bound is reset by every write that proceeds: a client that reads a large answer in
/// chunks, pausing 400 ms between them (under the head read time of 1 s), gets the whole answer,
/// though the answer takes several head read times to write. A `Stalling` that ran its bound
/// from the first blocked write and never reset it would cut this answer off.
#[test]
fn an_answer_read_in_chunks_with_short_pauses_is_written_whole() {
    const PAD: usize = 24 << 20;
    const CHUNK: usize = 3 << 20;
    let limits = Limits {
        head_read: Duration::from_secs(1),
        request_time: Duration::from_secs(10),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let request = command(address, &method, &path, "big");
    let reader = thread::spawn(move || {
        let mut stream = connect(address);
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("a read timeout");
        stream
            .write_all(request.as_bytes())
            .expect("the request is written");
        let started = Instant::now();
        let mut read = Vec::with_capacity(PAD + 4096);
        let mut chunk = vec![0u8; 64 << 10];
        let mut since_pause = 0usize;
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    read.extend_from_slice(&chunk[..count]);
                    since_pause += count;
                    if since_pause >= CHUNK {
                        since_pause = 0;
                        thread::sleep(Duration::from_millis(400));
                    }
                }
            }
        }
        (read, started.elapsed())
    });
    let exchange = listener.next(DUE).expect("the command is handed over");
    exchange.respond(Answer {
        status: 200,
        body: Some(json!({"pad": "x".repeat(PAD)})),
        headers: Vec::new(),
    });
    let (read, took) = reader.join().expect("the reader ends");
    let head = read
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("an answer head");
    let text = String::from_utf8_lossy(&read[..head]).into_owned();
    let length: usize = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .expect("a content length");
    assert!(text.starts_with("HTTP/1.1 200"), "{text}");
    assert_eq!(
        read.len() - head - 4,
        length,
        "the whole answer is written to a reader that pauses under the head read time; \
         {} of {length} bytes in {took:?}",
        read.len() - head - 4
    );
    assert!(
        took > Duration::from_secs(2),
        "the answer took more than one head read time to write: {took:?}"
    );
}

/// A command whose client left while it was queued is not handed once its request time has
/// passed. hyper drops a request's future when its client closes the connection, and with it the
/// only place the request time is enforced (`handed`), so the exchange stays queued with no
/// deadline: a later `Listener::next` hands a command 600 ms past its request time of 300 ms, and
/// the caller's thread commits a command nobody waits for. D3 A: "a request still queued answers
/// 503 ... and Listener::next skips it".
#[test]
fn a_command_whose_client_left_is_not_handed_past_its_request_time() {
    let limits = Limits {
        request_time: Duration::from_millis(300),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut client = connect(address);
    client
        .write_all(kept_command(address, &method, &path, "left").as_bytes())
        .expect("the request is written");
    thread::sleep(Duration::from_millis(100));
    let _ = client.shutdown(std::net::Shutdown::Both);
    drop(client);
    thread::sleep(Duration::from_millis(800));
    let handed = listener.next(Duration::from_millis(300));
    let late = handed
        .as_ref()
        .map(|exchange| String::from_utf8_lossy(exchange.body()).into_owned());
    assert!(
        late.is_none(),
        "a command queued 900 ms ago, past its request time of 300 ms, its client gone, is \
         handed to the caller's thread: {late:?}"
    );
}

/// A connection that waits at the cap with a whole command sent, when the run stops taking work
/// and the listener closes, is answered `503` like every request that reaches the listener from
/// then on (`Listener::stopping`, `Listener::close`), not reset: a reset after a whole `POST`
/// leaves its client not knowing whether the command ran. The listener drops its socket before
/// the places free up, so the kernel resets every connection in its backlog.
#[test]
fn a_connection_waiting_at_the_cap_when_the_listener_closes_is_answered_503() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(5),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut holder = connect(address);
    holder
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert_eq!(
        plain_answer(&mut holder, DUE).map(|(status, _)| status),
        Some(200),
        "the first connection is answered and kept alive"
    );
    let mut waiting = connect(address);
    waiting
        .write_all(command(address, &method, &path, "late").as_bytes())
        .expect("the request is written to the backlog");
    thread::sleep(Duration::from_millis(200));
    listener
        .stopping()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let closing = thread::spawn(move || listener.close(Duration::from_secs(2)));
    let answered = plain_answer(&mut waiting, DUE);
    closing.join().expect("the listener closes");
    drop(holder);
    assert_eq!(
        answered.as_ref().map(|(status, _)| *status),
        Some(503),
        "a whole command waiting at the cap when the run stopped taking work is answered: \
         {answered:?}"
    );
}

/// `Limits::head_read` says an idle or stalled connection holds a place of the cap for at most
/// the head read time. A connection that sends a head and stalls in its body holds it for
/// `BODY_READ` (10 s) whatever the head read time: with a cap of 1 and a head read time of 1 s,
/// a second client is not answered within 3 s.
#[test]
fn a_body_that_stalls_frees_its_place_at_the_head_read_time() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut stalled = connect(address);
    write!(
        stalled,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: 100\r\n\r\n{{\"desk_id\":"
    )
    .expect("a head and part of a body are written");
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    let answered = plain_answer(&mut waiting, DUE);
    let took = started.elapsed();
    drop(stalled);
    assert!(
        answered.is_some() && took < Duration::from_millis(2500),
        "a connection stalled in its body held the only place of the cap for {took:?} \
         (head read time 1 s): {answered:?}"
    );
}

// Round 3: against round 2's fixes: the deadline kept in the `Exchange` (F1), the backlog taken
// at close (F2) and the body read bounded by the head read time (F4), and what the documents say
// of them.

/// `Limits::head_read` says it bounds how long a request's body may *stall*, refused `408`, and
/// the changelog and `run --max-connections` say a connection "whose body stalls" is closed. A
/// body that never stalls, each piece arriving 100 ms after the one before, and that takes 1.6 s
/// in all, is refused `408` at the head read time of 1 s: the bound is on the whole body's
/// arrival, not on a stall.
#[test]
fn a_body_arriving_steadily_is_not_refused_as_stalled() {
    const PIECES: usize = 16;
    let limits = Limits {
        head_read: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let body = json!({"desk_id": "steady", "holder": "ada", "pad": "x".repeat(1600)}).to_string();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let client = thread::spawn(move || {
        let mut stream = connect(address);
        stream
            .write_all(head.as_bytes())
            .expect("the head is written");
        let bytes = body.into_bytes();
        let piece = bytes.len().div_ceil(PIECES);
        for chunk in bytes.chunks(piece) {
            thread::sleep(Duration::from_millis(100));
            if stream.write_all(chunk).is_err() {
                break;
            }
        }
        plain_answer(&mut stream, DUE)
    });
    let until = Instant::now() + Duration::from_secs(4);
    let mut handed = false;
    while !client.is_finished() && Instant::now() < until {
        if let Some(exchange) = listener.next(Duration::from_millis(50)) {
            handed = true;
            exchange.respond(Answer {
                status: 201,
                body: Some(json!({"opened": "steady"})),
                headers: Vec::new(),
            });
        }
    }
    let answered = client.join().expect("the client ends");
    assert!(
        handed && answered.as_ref().map(|(status, _)| *status) == Some(201),
        "a body that never paused for more than 100 ms, 1.6 s in all, is refused as stalled \
         at the head read time of 1 s (handed: {handed}): {answered:?}"
    );
}

/// A client still sending its body after its `408` does not hold its place: hyper gives up the
/// rest of the body and closes the connection once the `408` is written. With a cap of 1, a head
/// read time of 1 s and a client trickling 10 bytes every 50 ms into a body of 100,000 bytes, a
/// second client is answered within 2.5 s.
#[test]
fn a_client_still_sending_its_body_after_the_408_does_not_hold_the_cap() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(1),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut trickling = connect(address);
    write!(
        trickling,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: 100000\r\n\r\n{{\"desk_id\":\"t\",\"pad\":\""
    )
    .expect("a head and part of a body are written");
    let trickle = thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(4);
        while Instant::now() < until {
            if trickling.write_all(b"xxxxxxxxxx").is_err() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    });
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    let answered = plain_answer(&mut waiting, DUE);
    let took = started.elapsed();
    assert_eq!(
        answered.map(|(status, _)| status),
        Some(200),
        "a client trickling its body past its 408 held the only place for {took:?}"
    );
    assert!(
        took < Duration::from_millis(2500),
        "the client behind a trickling body waited {took:?}"
    );
    assert!(
        trickle.join().expect("the trickle ends"),
        "the trickling connection was closed"
    );
}

/// `run --max-connections` says: "When the run stops, each connection still waiting at the cap is
/// taken and answered `503`." A probe waiting at the cap, `GET /__spindle/live`, is taken once the
/// run stops taking work and the listener closes, and answered `200`.
// The coordinator set this contract in round 3: a probe answers as during the drain.
#[test]
fn a_probe_waiting_at_the_cap_when_the_run_stops_is_answered_as_during_the_drain() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(5),
        ..Limits::DEFAULT
    };
    let (surface, _, _) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut holder = connect(address);
    holder
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert_eq!(
        plain_answer(&mut holder, DUE).map(|(status, _)| status),
        Some(200),
        "the first connection is answered and kept alive"
    );
    let mut waiting = connect(address);
    waiting
        .write_all(get(address, LIVE).as_bytes())
        .expect("the probe is written to the backlog");
    thread::sleep(Duration::from_millis(200));
    listener
        .stopping()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let closing = thread::spawn(move || listener.close(Duration::from_secs(2)));
    let answered = plain_answer(&mut waiting, DUE);
    closing.join().expect("the listener closes");
    drop(holder);
    assert_eq!(
        answered.as_ref().map(|(status, _)| *status),
        Some(200),
        "a probe waiting at the cap when the run stopped: {answered:?}"
    );
}

/// `Listener::close`: the connections waiting in the backlog when it closes are taken, "and no
/// connection after them is accepted". The drain takes connections until an accept would wait,
/// so clients that keep connecting while it runs keep it accepting. With 16 threads connecting
/// to a closing listener and hanging up at once, the socket still takes connections more than
/// 1 s after `close` was called, and `close`, given a grace of 500 ms, has not returned.
#[test]
fn a_flood_of_connections_does_not_keep_a_closing_listener_accepting() {
    use std::sync::atomic::{AtomicBool, Ordering};
    const FLOODERS: usize = 16;
    let (surface, _, _) = desk(Limits::DEFAULT);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let origin = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let flood: Vec<_> = (0..FLOODERS)
        .map(|_| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut last = None;
                let mut made = 0u64;
                while !stop.load(Ordering::SeqCst) && origin.elapsed() < Duration::from_secs(6) {
                    match TcpStream::connect_timeout(&address, Duration::from_millis(500)) {
                        Ok(stream) => {
                            last = Some(origin.elapsed());
                            made += 1;
                            drop(stream);
                        }
                        Err(_) => thread::sleep(Duration::from_millis(1)),
                    }
                }
                (last, made)
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(300));
    let called = origin.elapsed();
    let closing = thread::spawn(move || {
        listener.close(Duration::from_millis(500));
        origin.elapsed()
    });
    let until = Instant::now() + Duration::from_secs(4);
    while !closing.is_finished() && Instant::now() < until {
        thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, Ordering::SeqCst);
    let returned = closing.join().expect("the listener closes");
    let (last, made) = flood
        .into_iter()
        .map(|flooder| flooder.join().expect("the flooder ends"))
        .fold((None, 0u64), |(last, made), (one, count)| {
            (last.max(one), made + count)
        });
    let last = last.expect("some connection was made");
    assert!(
        returned.saturating_sub(called) < Duration::from_secs(2)
            && last.saturating_sub(called) < Duration::from_secs(1),
        "close was called at {called:?} and returned at {returned:?}; the socket took a \
         connection at {last:?} ({made} connections in all)"
    );
}

/// A client in the backlog that sends its request slowly while the listener closes is answered:
/// the head arrives in two parts 500 ms apart, inside the grace of 3 s and the head read time of
/// 5 s, and is answered `503`, the run having stopped taking work.
#[test]
fn a_request_sent_slowly_from_the_backlog_during_the_close_is_answered() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(5),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut holder = connect(address);
    holder
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert_eq!(
        plain_answer(&mut holder, DUE).map(|(status, _)| status),
        Some(200),
        "the first connection is answered and kept alive"
    );
    let request = command(address, &method, &path, "slow");
    let (first, rest) = request.split_at(request.len() / 3);
    let mut waiting = connect(address);
    waiting
        .write_all(first.as_bytes())
        .expect("the first part is written to the backlog");
    thread::sleep(Duration::from_millis(100));
    listener
        .stopping()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let closing = thread::spawn(move || listener.close(Duration::from_secs(3)));
    thread::sleep(Duration::from_millis(500));
    waiting
        .write_all(rest.as_bytes())
        .expect("the rest is written during the close");
    let answered = plain_answer(&mut waiting, DUE);
    closing.join().expect("the listener closes");
    drop(holder);
    assert_eq!(
        answered.as_ref().map(|(status, _)| *status),
        Some(503),
        "a request sent slowly from the backlog while the listener closes: {answered:?}"
    );
}

/// A connection taken from the backlog at close is not kept alive: of two requests it pipelined,
/// the first is answered `503` and the connection then closes, so it holds the close no longer
/// than its one answer.
#[test]
fn a_connection_taken_from_the_backlog_at_close_answers_one_request_and_closes() {
    let limits = Limits {
        connections: 1,
        head_read: Duration::from_secs(5),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut holder = connect(address);
    holder
        .write_all(get(address, LIVE).as_bytes())
        .expect("the request is written");
    assert_eq!(
        plain_answer(&mut holder, DUE).map(|(status, _)| status),
        Some(200),
        "the first connection is answered and kept alive"
    );
    let mut waiting = connect(address);
    let twice = format!(
        "{}{}",
        kept_command(address, &method, &path, "one"),
        kept_command(address, &method, &path, "two")
    );
    waiting
        .write_all(twice.as_bytes())
        .expect("two requests are written to the backlog");
    thread::sleep(Duration::from_millis(200));
    listener
        .stopping()
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let started = Instant::now();
    let closing = thread::spawn(move || listener.close(Duration::from_secs(4)));
    let first = plain_answer(&mut waiting, DUE);
    let second = plain_answer(&mut waiting, Duration::from_secs(1));
    closing.join().expect("the listener closes");
    let took = started.elapsed();
    drop(holder);
    assert_eq!(first.as_ref().map(|(status, _)| *status), Some(503));
    assert!(second.is_none(), "a second answer on it: {second:?}");
    assert!(
        took < Duration::from_millis(2500),
        "the close waited {took:?} with a grace of 4 s"
    );
}

/// No command is both handed and refused `503` at a tight request time: 40 commands at once,
/// each with 30 ms, taken by a thread that pauses 0 to 9 ms after each (a fixed sequence), so
/// that takes fall on both sides of their deadlines. Each handed answers `201` or `504`, each not
/// handed `503` with `committed: false`, and every command is one or the other.
#[test]
fn no_command_is_both_handed_and_refused_at_a_tight_request_time() {
    const SENT: usize = 40;
    let limits = Limits {
        request_time: Duration::from_millis(30),
        ..Limits::DEFAULT
    };
    let (surface, method, path) = desk(limits);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let clients: Vec<_> = (0..SENT)
        .map(|index| {
            let request = command(address, &method, &path, &format!("t-{index}"));
            thread::spawn(move || {
                let mut stream = connect(address);
                stream
                    .write_all(request.as_bytes())
                    .expect("the request is written");
                (format!("t-{index}"), plain_answer(&mut stream, DUE))
            })
        })
        .collect();
    let mut handed = BTreeSet::new();
    let mut pause = 7u64;
    let until = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < until {
        if let Some(exchange) = listener.next(Duration::from_millis(10)) {
            let body: Value = serde_json::from_slice(exchange.body()).expect("a JSON body");
            let desk_id = body["desk_id"].as_str().expect("a desk id").to_owned();
            assert!(handed.insert(desk_id.clone()), "{desk_id} handed twice");
            pause = (pause * 31 + 11) % 97;
            thread::sleep(Duration::from_millis(pause % 10));
            exchange.respond(Answer {
                status: 201,
                body: Some(json!({"opened": desk_id})),
                headers: Vec::new(),
            });
        }
    }
    let answers: BTreeMap<String, Option<(u16, String)>> = clients
        .into_iter()
        .map(|client| client.join().expect("the client's thread ends"))
        .collect();
    let mut refused = 0;
    for (desk_id, answered) in &answers {
        let (status, body) = answered
            .clone()
            .unwrap_or_else(|| panic!("{desk_id} is answered"));
        if handed.contains(desk_id) {
            assert!(
                status == 201 || status == 504,
                "{desk_id} was handed and answered {status}: {body}"
            );
        } else {
            refused += 1;
            assert_eq!(status, 503, "{desk_id} was never handed: {body}");
            let body: Value = serde_json::from_str(&body).expect("a JSON refusal");
            assert_eq!(body["committed"], json!(false), "{desk_id}: {body}");
        }
    }
    assert_eq!(
        handed.len() + refused,
        SENT,
        "every command is accounted for"
    );
}
