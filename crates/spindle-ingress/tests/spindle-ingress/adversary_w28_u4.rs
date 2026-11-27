//! Adversary cases of wave 28 unit U4 (`story:health-and-graceful-drain`) at the listener: once
//! the stopping flag is set, a request whose body was still arriving is answered `503` and never
//! handed to the caller's thread, and the health routes still answer, readiness `503`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::Value;
use spindle_ingress::{HTTP_LOOPBACK, Listener, Surface};

/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn surface() -> Surface {
    let fixtures: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let ir = spindle_spec::load(&fixtures.join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    Surface::new(&ir, &["desk".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

/// Every byte answered on `stream` until the server closes it.
fn rest(mut stream: TcpStream) -> String {
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    String::from_utf8_lossy(&answer).into_owned()
}

/// `GET <path>` on its own connection: the status and the JSON body.
fn get(address: SocketAddr, path: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let answer = rest(stream);
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// A reset whose head and first body bytes arrived before the run stopped taking work, and whose
/// last bytes arrive after: it reads `503`, and the caller's thread is never handed it. Kills the
/// mutant that drops the stopping check in `handed`.
#[test]
fn a_body_finished_after_the_stop_reads_503_and_is_never_handed_over() {
    let listener = Listener::bind(0, surface()).expect("the listener binds");
    let address = listener.local_addr();
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = "{\"rows\": []}";
    write!(
        stream,
        "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        &body[..2]
    )
    .expect("the head is written");
    std::thread::sleep(Duration::from_millis(300));
    listener.stopping().store(true, Ordering::SeqCst);
    write!(stream, "{}", &body[2..]).expect("the body is finished");
    let answer = rest(stream);
    assert!(
        answer.starts_with("HTTP/1.1 503") && answer.contains("the run is stopping"),
        "a body finished after the stop: {answer:?}"
    );
    assert!(
        listener.next(Duration::from_millis(200)).is_none(),
        "the request was handed to the caller's thread"
    );
}

/// Once stopping, on a surface with no seed: readiness reads `503` naming the stop, the seed reads
/// `none`, liveness still `200`, and a console route arriving reads `503`.
#[test]
fn the_health_routes_answer_once_stopping() {
    let listener = Listener::bind(0, surface()).expect("the listener binds");
    let address = listener.local_addr();
    assert!(listener.next(Duration::ZERO).is_none());
    let (status, body) = get(address, "/__spindle/ready");
    assert_eq!(status, 200, "{body}");
    listener.stopping().store(true, Ordering::SeqCst);
    let (status, body) = get(address, "/__spindle/ready");
    assert_eq!(status, 503, "{body}");
    assert_eq!(
        body,
        serde_json::json!({"ready": false, "seed": "none", "stopping": true})
    );
    let (status, body) = get(address, "/__spindle/seed");
    assert_eq!((status, body), (200, serde_json::json!({"state": "none"})));
    let (status, body) = get(address, "/__spindle/live");
    assert_eq!(status, 200, "{body}");
    let (status, body) = get(address, "/api/model");
    assert_eq!(status, 503, "{body}");
    assert!(listener.next(Duration::from_millis(100)).is_none());
}
