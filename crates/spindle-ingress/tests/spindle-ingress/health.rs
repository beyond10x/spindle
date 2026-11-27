//! A listener's liveness route (`story:health-and-graceful-drain`): `GET /__spindle/live` answers
//! `200` while the caller's thread holds a request it took or asks for requests
//! ([`Listener::next`]) within the listener's bound and `503` once it has done neither, with or
//! without a recorder, on the listener's own thread; on a
//! surface whose requests prove their caller it requires the proxy's secret, as every
//! `/__spindle/` route does.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use spindle_ingress::{HTTP_LOOPBACK, Listener, Surface};

/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);
/// The liveness bound the tests give the listener.
const BOUND: Duration = Duration::from_millis(300);
const HEADER: &str = "X-Spindle-Caller";
const SECRET: &str = "health-proxy-secret-7e2a";

fn desk() -> spindle_spec::EssIr {
    let fixtures: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    spindle_spec::load(&fixtures.join("desk"))
        .expect("the desk fixture compiles")
        .ir
}

fn surface() -> Surface {
    Surface::new(&desk(), &["desk".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

/// `GET <path>` on its own connection, with `headers`: the status and the body.
fn get(address: SocketAddr, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{extra}\r\n"
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
    (status, body.to_owned())
}

/// `GET /__spindle/live`: the status and the JSON body.
fn live(address: SocketAddr, headers: &[(&str, &str)]) -> (u16, Value) {
    let (status, body) = get(address, "/__spindle/live", headers);
    let body = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// Right after the caller asked for a request the route answers `200`; once the caller has not
/// asked for longer than the bound, `503`; once it asks again, `200` again.
#[test]
fn liveness_answers_200_while_the_host_answers() {
    let listener = Listener::bind(0, surface())
        .expect("the listener binds")
        .live_within(BOUND);
    let address = listener.local_addr();
    assert!(listener.next(Duration::ZERO).is_none());
    let (status, body) = live(address, &[]);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["live"], true, "{body}");
    assert_eq!(body["bound_ms"], 300, "{body}");

    std::thread::sleep(BOUND * 2);
    let (status, body) = live(address, &[]);
    assert_eq!(
        status, 503,
        "the caller asked nothing for twice the bound: {body}"
    );
    assert_eq!(body["live"], false, "{body}");
    assert!(
        body["idle_ms"].as_u64().is_some_and(|idle| idle >= 300),
        "{body}"
    );

    assert!(listener.next(Duration::from_millis(10)).is_none());
    let (status, body) = live(address, &[]);
    assert_eq!(status, 200, "the caller asks again: {body}");
}

/// While the caller holds a request it took from [`Listener::next`], answering it past the bound,
/// the route answers `200`, as a command held by an external call or a reset seeding again does;
/// once the caller has asked again, holding none, and then stays silent past the bound, `503`.
#[test]
fn liveness_holds_while_a_taken_request_is_answered() {
    let listener = Listener::bind(0, surface())
        .expect("the listener binds")
        .live_within(BOUND);
    let address = listener.local_addr();
    let client = std::thread::spawn(move || {
        let mut stream =
            TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        write!(
            stream,
            "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Length: 0\r\n\r\n"
        )
        .expect("the reset is written");
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
    });
    let exchange = listener.next(ANSWER).expect("the reset is handed over");
    std::thread::sleep(BOUND * 2);
    let (status, body) = live(address, &[]);
    assert_eq!(status, 200, "a request held past the bound: {body}");
    assert_eq!(body["live"], true, "{body}");
    exchange.respond(spindle_ingress::Answer::refusal(
        400,
        "answered by the test",
    ));
    client.join().expect("the client ends");

    assert!(listener.next(Duration::ZERO).is_none());
    std::thread::sleep(BOUND * 2);
    let (status, body) = live(address, &[]);
    assert_eq!(status, 503, "idle and silent past the bound: {body}");
}

/// A listener standing by for another process's lease (`story:several-writers-one-postgres`),
/// bound before any host exists: `GET /__spindle/live` answers `200` while its caller polls,
/// `GET /__spindle/ready` `503` with `standby: true`, and every other request `503` at once,
/// none of them handed to the caller.
#[test]
fn a_standby_reads_not_ready() {
    let listener =
        Listener::bind_standing_by_on(std::net::Ipv4Addr::LOCALHOST.into(), 0, surface(), None)
            .expect("the listener binds")
            .live_within(BOUND);
    let address = listener.local_addr();
    assert!(listener.next(Duration::ZERO).is_none());
    let (status, body) = live(address, &[]);
    assert_eq!(status, 200, "a standby polling is live: {body}");
    let (status, body) = get(address, "/__spindle/ready", &[]);
    let body: Value = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"));
    assert_eq!(status, 503, "a standby is not ready: {body}");
    assert_eq!(
        body,
        serde_json::json!({"ready": false, "seed": "none", "stopping": false, "standby": true})
    );
    for (method, path) in [("GET", "/api/model"), ("POST", "/api/reset")] {
        let mut stream =
            TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Length: 0\r\n\r\n"
        )
        .expect("the request is written");
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .expect("the answer is read before the timeout");
        assert!(
            answer.starts_with("HTTP/1.1 503"),
            "{method} {path} on a standby: {answer}"
        );
        assert!(answer.contains("standby"), "{method} {path}: {answer}");
    }
    assert!(
        listener.next(Duration::ZERO).is_none(),
        "no request reaches the standby's caller"
    );
}

/// The route answers on a surface that records nothing, whose other `/__spindle/` paths answer
/// `404`; on one whose requests prove their caller it answers `403` without the proxy's secret
/// and `200` with it.
#[test]
fn the_liveness_route_needs_no_observe() {
    let listener = Listener::bind(0, surface()).expect("the listener binds");
    let address = listener.local_addr();
    let (status, body) = live(address, &[]);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["live"], true, "{body}");
    let (status, _) = get(address, "/__spindle/requests", &[]);
    assert_eq!(status, 404, "the surface records nothing");

    let proving = Surface::proving_callers(&desk(), &["desk".to_owned()], None, HEADER, SECRET)
        .expect("the desk surface proves its callers");
    let listener = Listener::bind(0, proving).expect("the listener binds");
    let address = listener.local_addr();
    let (status, body) = get(address, "/__spindle/live", &[]);
    assert_eq!(status, 403, "without the proxy's secret: {body}");
    let (status, body) = live(address, &[(Surface::SECRET_HEADER, SECRET)]);
    assert_eq!(status, 200, "with the proxy's secret: {body}");
}
