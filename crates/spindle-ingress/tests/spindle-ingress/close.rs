//! [`Listener::close`] (`story:serve-loop-signal-exit-answers`): closing the listener stops it
//! taking requests, answers each request still queued for the caller's thread `503`, and writes
//! every answer the caller handed back before the server thread ends, so no client of a run that
//! stops reads a closed connection where an answer was due.
//!
//! The desk fixture's surface with the console routes: every `POST /api/reset` is handed to the
//! caller's thread as an [`Exchange`](spindle_ingress::Exchange), which the test answers itself.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::json;
use spindle_ingress::{Answer, Console, HTTP_LOOPBACK, Listener, Surface};

/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);
/// How long [`Listener::close`] waits for the answers handed back to be written.
const GRACE: Duration = Duration::from_secs(5);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn surface() -> Surface {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    Surface::new(&ir, &["desk".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"))
}

/// `POST /api/reset` with `body` on its own connection, from a thread of its own: every byte
/// answered, empty where the connection closed without an answer.
fn reset(address: SocketAddr, body: &'static str) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, ANSWER) else {
            return String::new();
        };
        let _ = stream.set_read_timeout(Some(ANSWER));
        let written = write!(
            stream,
            "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        if written.is_err() {
            return String::new();
        }
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
        String::from_utf8_lossy(&answer).into_owned()
    })
}

/// A reset refused `400` and handed back, then the listener closed at once: the client reads the
/// `400`, never a closed connection. Twenty listeners, each closed right after its answer.
#[test]
fn a_refused_answer_handed_before_close_is_written() {
    for round in 0..20 {
        let listener = Listener::bind(0, surface()).expect("the listener binds");
        let client = reset(listener.local_addr(), "{\"rows\": 7}");
        let exchange = listener.next(ANSWER).expect("the reset is handed over");
        assert_eq!(exchange.console(), Some(Console::Reset));
        exchange.respond(Answer::refusal(400, "the world is refused"));
        listener.close(GRACE);
        let answer = client.join().expect("the client's thread ends");
        assert!(
            answer.starts_with("HTTP/1.1 400") && answer.contains("the world is refused"),
            "round {round}: the refusal handed back before the close is written: {answer:?}"
        );
    }
}

/// Eight resets taken and answered, four more still queued, then the listener closed: each of the
/// eight reads its answer, each of the four reads `503`.
#[test]
fn a_closed_listener_writes_every_answer_it_was_handed() {
    const HANDED: usize = 8;
    const QUEUED: usize = 4;
    let listener = Listener::bind(0, surface()).expect("the listener binds");
    let address = listener.local_addr();
    let handed: Vec<_> = (0..HANDED).map(|_| reset(address, "")).collect();
    for _ in 0..HANDED {
        let exchange = listener.next(ANSWER).expect("a reset is handed over");
        exchange.respond(Answer {
            status: 200,
            body: Some(json!({"emptied": ["desk"]})),
            headers: Vec::new(),
        });
    }
    let queued: Vec<_> = (0..QUEUED).map(|_| reset(address, "")).collect();
    // Each of the four reaches the queue while no one takes from it.
    std::thread::sleep(Duration::from_millis(500));
    listener.close(GRACE);
    for client in handed {
        let answer = client.join().expect("the client's thread ends");
        assert!(
            answer.starts_with("HTTP/1.1 200") && answer.contains("emptied"),
            "an answer handed back before the close is written: {answer:?}"
        );
    }
    for client in queued {
        let answer = client.join().expect("the client's thread ends");
        assert!(
            answer.starts_with("HTTP/1.1 503"),
            "a request still queued at the close reads 503: {answer:?}"
        );
    }
}
