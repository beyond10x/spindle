//! A served run's readiness on SIGTERM and its drain (`story:health-and-graceful-drain`): once the
//! signal arrives `GET /__spindle/ready` answers `503` at once, a request arriving from then on is
//! answered `503` at once, the request in flight finishes, and the requests already waiting are
//! answered with their outcomes for up to `--drain` seconds before the run exits 143.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitStatus;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, GRANT, OPERATOR, Served, VOUCH, environment, exchange, grant, holding_desk, route,
    scratch, serve, write,
};

/// The answer of one HTTP/1.1 `method` on `path` with `body`, on its own connection from a thread
/// of its own: its status, `None` where the connection ended without an answer head, and when it
/// arrived.
fn later(
    address: SocketAddr,
    (method, path): (String, String),
    body: Value,
) -> mpsc::Receiver<(Option<u16>, Instant)> {
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let body = body.to_string();
        let status = (|| {
            let mut stream = TcpStream::connect_timeout(&address, ANSWER).ok()?;
            stream.set_read_timeout(Some(ANSWER)).ok()?;
            write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .ok()?;
            let mut answer = Vec::new();
            let _ = stream.read_to_end(&mut answer);
            String::from_utf8_lossy(&answer)
                .split(' ')
                .nth(1)
                .and_then(|status| status.parse().ok())
        })();
        let _ = answered.send((status, Instant::now()));
    });
    answer
}

fn terminate(served: &Served) {
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("the signal is sent");
}

/// The exit status of `served` once it exits by itself within `within`, and when it did; `None`
/// where it is still running then.
fn exited_within(served: &mut Served, within: Duration) -> Option<(ExitStatus, Instant)> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = served.child.try_wait().expect("the run is polled") {
            return Some((status, Instant::now()));
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A run serving every command as the operator of `north`, `VouchFor` held at `desk`.
fn served(directory: &tempfile::TempDir, desk: SocketAddr, extra: &[&str]) -> Served {
    let document = environment(directory, "memory", Some(desk), "");
    let caller = json!({"tenant": "north"}).to_string();
    let mut arguments = vec![
        "--environment",
        &document,
        "--fixed-actor",
        OPERATOR,
        "--fixed-caller",
        &caller,
    ];
    arguments.extend_from_slice(extra);
    serve(&arguments)
}

fn vouching(voucher: &str) -> Value {
    json!({"voucher_id": voucher, "role_id": "admin"})
}

fn granting(role: &str) -> Value {
    json!({"role_id": role, "title": role})
}

/// The status `GET /__spindle/ready` answers.
fn readiness(address: SocketAddr) -> (u16, String) {
    exchange(address, "GET", "/__spindle/ready", &[])
}

/// A run whose seed ended reads `200`; a command is held at the desk when SIGTERM arrives, and
/// readiness reads `503` while it is still held. Once the desk answers, the command reads its
/// outcome and the run exits 143.
#[test]
fn readiness_reads_503_once_sigterm_arrives() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let mut served = served(&directory, desk, &["--seed", &seed]);
    served.ended();
    let (status, body) = readiness(served.address);
    assert_eq!(status, 200, "{body}");

    let held = later(served.address, route(VOUCH), vouching("v-1"));
    asked
        .recv_timeout(ANSWER)
        .expect("the command asks the desk");
    terminate(&served);
    let deadline = Instant::now() + ANSWER;
    let body = loop {
        let (status, body) = readiness(served.address);
        if status == 503 {
            break body;
        }
        assert!(
            Instant::now() < deadline,
            "readiness still reads {status} after SIGTERM: {body}"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let body: Value = serde_json::from_str(&body).expect("readiness answers JSON");
    assert_eq!(body["stopping"], true, "{body}");
    assert!(
        held.try_recv().is_err(),
        "the command is still held while readiness reads 503"
    );
    release.send(()).expect("the desk is released");
    let (status, _) = held.recv_timeout(ANSWER).expect("the command answers");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "{status:?}"
    );
    let (status, _) = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143));
}

/// One command held at the desk and one queued when SIGTERM arrives are both answered with their
/// outcomes; a request sent after the signal is answered `503` while the first is still held; the
/// run exits 143 within the drain bound.
#[test]
fn sigterm_answers_the_requests_in_flight() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let mut served = served(&directory, desk, &["--drain", "5"]);
    let held = later(served.address, route(VOUCH), vouching("v-1"));
    asked
        .recv_timeout(ANSWER)
        .expect("the command asks the desk");
    let queued = later(served.address, route(GRANT), granting("queued"));
    // The grant reaches the loop's queue while the loop waits for the desk.
    std::thread::sleep(Duration::from_millis(300));
    terminate(&served);
    let signalled = Instant::now();
    std::thread::sleep(Duration::from_millis(200));
    let late = later(served.address, route(GRANT), granting("late"));
    let (status, _) = late.recv_timeout(ANSWER).expect("the late request answers");
    assert_eq!(status, Some(503), "a request sent after the signal");
    assert!(
        held.try_recv().is_err(),
        "the late request is answered at once, while the first is still held"
    );
    release.send(()).expect("the desk is released");
    let (status, _) = held.recv_timeout(ANSWER).expect("the held command answers");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "the command in flight: {status:?}"
    );
    let (status, _) = queued
        .recv_timeout(ANSWER)
        .expect("the queued grant answers");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "the request queued at the signal: {status:?}"
    );
    let (status, at) = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143));
    assert!(
        at.duration_since(signalled) < Duration::from_secs(5),
        "the run exits within the drain bound: {:?}",
        at.duration_since(signalled)
    );
}

/// With `--drain 1`, the queued command the loop takes first is held past the bound and still
/// finishes; the grant queued behind it is answered `503`, and the run exits 143.
#[test]
fn a_request_queued_past_the_drain_bound_reads_503() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let mut served = served(&directory, desk, &["--drain", "1"]);
    let first = later(served.address, route(VOUCH), vouching("v-1"));
    asked
        .recv_timeout(ANSWER)
        .expect("the first command asks the desk");
    let second = later(served.address, route(VOUCH), vouching("v-2"));
    std::thread::sleep(Duration::from_millis(200));
    let third = later(served.address, route(GRANT), granting("past"));
    std::thread::sleep(Duration::from_millis(200));
    terminate(&served);
    std::thread::sleep(Duration::from_millis(100));
    release.send(()).expect("the desk is released");
    let (status, _) = first
        .recv_timeout(ANSWER)
        .expect("the first command answers");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "{status:?}"
    );
    asked
        .recv_timeout(ANSWER)
        .expect("the drain takes the second command, which asks the desk");
    std::thread::sleep(Duration::from_millis(1500));
    release.send(()).expect("the desk is released");
    let (status, _) = second
        .recv_timeout(ANSWER)
        .expect("the second command answers");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "the request taken within the bound finishes past it: {status:?}"
    );
    let (status, _) = third.recv_timeout(ANSWER).expect("the grant answers");
    assert_eq!(
        status,
        Some(503),
        "the request still queued when the bound passed"
    );
    let (status, _) = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143));
}
