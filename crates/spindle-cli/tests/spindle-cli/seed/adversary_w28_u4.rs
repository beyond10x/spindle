//! Adversary cases of wave 28 unit U4 (`story:seed-readiness`, `story:health-and-graceful-drain`):
//! the health routes' guard on a `--trusted-caller-header` run, readiness against a reset that
//! seeds again, the drain of a queued reset, the exit status on SIGINT with the default drain, and
//! a drain bound past what the clock can count.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, GRANT, HEADER, OPERATOR, SECRET, SECRET_HEADER, Served, VISITOR, VOUCH, environment,
    exchange, grant, holding_desk, role_ids, route, scratch, serve, write,
};

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

fn signal(served: &Served, signal: rustix::process::Signal) {
    rustix::process::kill_process(rustix::process::Pid::from_child(&served.child), signal)
        .expect("the signal is sent");
}

/// The exit status of `served` once it exits by itself within `within`; `None` while it runs.
fn exited_within(served: &mut Served, within: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = served.child.try_wait().expect("the run is polled") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One HTTP/1.1 `method` on `path` with the JSON `body`, on its own connection from a thread of
/// its own: the status, `None` where the connection ended without an answer head.
fn later(
    address: SocketAddr,
    (method, path): (String, String),
    body: String,
) -> JoinHandle<Option<u16>> {
    std::thread::spawn(move || {
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
    })
}

/// `POST /api/reset` with no body, from a thread of its own.
fn reset_later(address: SocketAddr) -> JoinHandle<Option<u16>> {
    later(
        address,
        ("POST".to_owned(), "/api/reset".to_owned()),
        String::new(),
    )
}

/// Reads `GET <path>` over and over on kept-alive connections until `stop` is set: every answer
/// that names the seed `ended`, with its status.
fn hammer(
    address: SocketAddr,
    path: &'static str,
    stop: Arc<AtomicBool>,
) -> JoinHandle<Vec<String>> {
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            let Ok(mut stream) = TcpStream::connect_timeout(&address, ANSWER) else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(ANSWER));
            let _ = stream.set_nodelay(true);
            while !stop.load(Ordering::SeqCst) {
                if write!(stream, "GET {path} HTTP/1.1\r\nHost: {address}\r\n\r\n").is_err() {
                    break;
                }
                let Some((status, body, close)) = answered(&mut stream) else {
                    break;
                };
                if body.contains("\"ended\"") {
                    seen.push(format!("{status} {body}"));
                }
                if close {
                    break;
                }
            }
        }
        seen
    })
}

/// One answer read from `stream`: its status, its body and whether the server closes the
/// connection after it.
fn answered(stream: &mut TcpStream) -> Option<(u16, String, bool)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let status = head.split(' ').nth(1)?.parse().ok()?;
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).ok()?;
    let close = head.contains("connection: close");
    Some((status, String::from_utf8_lossy(&body).into_owned(), close))
}

/// On a `--trusted-caller-header` run every health route, `/__spindle/live` included, answers
/// `403` without the proxy's secret and with a wrong one, and answers with it.
#[test]
fn every_health_route_answers_403_without_the_proxy_secret() {
    let directory = scratch();
    let secret = write(&directory, "proxy-secret", &format!("{SECRET}\n"));
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "west", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
        "--trusted-caller-header",
        HEADER,
        "--trusted-caller-secret",
        &secret,
    ]);
    served.ended();
    for path in ["/__spindle/live", "/__spindle/ready", "/__spindle/seed"] {
        let (status, body) = exchange(served.address, "GET", path, &[]);
        assert_eq!(status, 403, "{path} without the secret: {body}");
        let (status, body) = exchange(
            served.address,
            "GET",
            path,
            &[(SECRET_HEADER, "seed-proxy-secret-4c1e")],
        );
        assert_eq!(status, 403, "{path} with a wrong secret: {body}");
        let (status, body) = exchange(served.address, "GET", path, &[(SECRET_HEADER, SECRET)]);
        assert_eq!(status, 200, "{path} with the secret: {body}");
    }
}

/// The start's seed is held at its second entry when a reset with no world is queued; once the
/// desk lets go the loop takes the reset, which empties the stores and seeds them again from the
/// first entry, held again at the second. `spindle.observe.SeedState` says such a seed run reads
/// `running` from before the reset empties the stores until it ends, and no request reads the run
/// ready while its stores are empty: from the reset's queueing until its seed is held again,
/// `GET /__spindle/ready` reads `503` only. Kills the publish of the cut run as `ended`
/// (`Seeding::cut`) between the reset's emptying and `Seeding::restart`.
#[test]
fn a_plain_reset_never_reads_ready_before_its_seed_runs_again() {
    const ROUNDS: usize = 6;
    const PROBES: usize = 4;
    let mut seen = Vec::new();
    for round in 0..ROUNDS {
        let directory = scratch();
        let state = directory.path().join("state");
        let state = state.to_str().expect("a UTF-8 path");
        let (desk, asked, release) = holding_desk();
        let seed = write(
            &directory,
            "seed.json",
            &json!([
                grant("admin", "north", "seed-1"),
                vouch("v-1", "seed-2"),
                grant("auditor", "south", "seed-3"),
            ])
            .to_string(),
        );
        let document = environment(&directory, "sqlite", Some(desk), "");
        let mut served = serve(&[
            "--environment",
            &document,
            "--state",
            state,
            "--seed",
            &seed,
        ]);
        asked.recv_timeout(ANSWER).expect("the start's seed asks");
        let stop = Arc::new(AtomicBool::new(false));
        let probes: Vec<_> = (0..PROBES)
            .map(|probe| {
                let path = if probe % 2 == 0 {
                    "/__spindle/ready"
                } else {
                    "/__spindle/seed"
                };
                hammer(served.address, path, Arc::clone(&stop))
            })
            .collect();
        let answer = reset_later(served.address);
        std::thread::sleep(Duration::from_millis(300));
        release.send(()).expect("the desk is released");
        asked.recv_timeout(ANSWER).expect("the reset's seed asks");
        stop.store(true, Ordering::SeqCst);
        for probe in probes {
            for answer in probe.join().expect("the probe ends") {
                seen.push(format!("round {round}: {answer}"));
            }
        }
        release.send(()).expect("the desk is released");
        assert_eq!(
            answer.join().expect("the reset's client ends"),
            Some(200),
            "round {round}: the reset answers"
        );
        let first = served.ended();
        assert!(
            first.contains(": 2 sent,"),
            "round {round}: the start's seed never ended by itself, the reset cut it: {first}"
        );
        if !seen.is_empty() {
            break;
        }
    }
    assert!(
        seen.is_empty(),
        "the seed read `ended`, readiness 200, after the reset was queued and before its seed \
         ran again: {seen:#?}"
    );
}

/// A command is held at the desk, and a reset with no world queued behind it, when SIGTERM
/// arrives; the default drain takes the reset once the command is answered. The reset answers
/// with the seed's effects in place (`story:reset-reapplies-seed`), so it either reads `503`,
/// never taken, or a restart on the state without `--seed` holds the seed's row. Before this unit
/// the queued reset read `503` (`signal_exit::a_request_queued_at_the_signal_reads_503`, now run
/// with `--drain 0`).
#[test]
fn a_reset_taken_in_the_drain_answers_with_its_seed_in_place() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "sqlite", Some(desk), "");
    let caller = json!({"tenant": "north"}).to_string();
    let mut served = serve(&[
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--fixed-actor",
        OPERATOR,
        "--fixed-caller",
        &caller,
    ]);
    served.ended();
    let held = later(
        served.address,
        route(VOUCH),
        json!({"voucher_id": "v-1", "role_id": "admin"}).to_string(),
    );
    asked
        .recv_timeout(ANSWER)
        .expect("the command asks the desk");
    let reset = reset_later(served.address);
    std::thread::sleep(Duration::from_millis(300));
    signal(&served, rustix::process::Signal::TERM);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    let status = held.join().expect("the held command's client ends");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "the command in flight: {status:?}"
    );
    let reset = reset.join().expect("the reset's client ends");
    let exited = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(exited.code(), Some(143));
    served.stop();

    let restarted = serve(&["--environment", &document, "--state", state]);
    let held = role_ids(&restarted);
    assert!(
        reset == Some(503) || held == ["admin"],
        "the reset queued at SIGTERM answered {reset:?} and left the state holding {held:?}: \
         its stores emptied and its seed never sent again"
    );
}

/// SIGINT with the default drain while a command is held and a grant is queued: both are answered
/// with their outcomes, and the run exits 130.
#[test]
fn sigint_with_the_default_drain_answers_the_queued_request_and_exits_130() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let document = environment(&directory, "memory", Some(desk), "");
    let caller = json!({"tenant": "north"}).to_string();
    let mut served = serve(&[
        "--environment",
        &document,
        "--fixed-actor",
        OPERATOR,
        "--fixed-caller",
        &caller,
    ]);
    let held = later(
        served.address,
        route(VOUCH),
        json!({"voucher_id": "v-1", "role_id": "admin"}).to_string(),
    );
    asked
        .recv_timeout(ANSWER)
        .expect("the command asks the desk");
    let queued = later(
        served.address,
        route(GRANT),
        json!({"role_id": "queued", "title": "queued"}).to_string(),
    );
    std::thread::sleep(Duration::from_millis(300));
    signal(&served, rustix::process::Signal::INT);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    for (what, client) in [("held", held), ("queued", queued)] {
        let status = client.join().expect("the client ends");
        assert!(
            status.is_some_and(|status| (200..300).contains(&status)),
            "the {what} request: {status:?}"
        );
    }
    let exited = exited_within(&mut served, ANSWER).expect("the run exits on SIGINT");
    assert_eq!(exited.code(), Some(130));
}

/// `--drain` takes any `u64`; given one past what the monotonic clock can add, SIGTERM still ends
/// the run with 143, not a panic.
#[test]
fn a_drain_bound_past_the_clock_still_exits_143() {
    let directory = scratch();
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--answer-default",
        "--drain",
        "18446744073709551615",
    ]);
    signal(&served, rustix::process::Signal::TERM);
    let exited = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    let code = exited.code();
    let stderr = served.stop();
    assert_eq!(code, Some(143), "{stderr:#?}");
}
