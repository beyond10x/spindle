//! The serve loop's exit on SIGTERM or SIGINT (`story:serve-loop-signal-exit-answers`): once the
//! signal arrives the loop takes no further exchange, a request still queued for it is answered
//! `503`, and every answer already handed to the listener, a seeded reset's included, is written
//! before the run exits.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, Served, VISITOR, VOUCH, environment, grant, holding_desk, published, role_ids, scratch,
    serve,
};

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

/// `POST /api/reset` with no body, on its own connection, from a thread of its own: the status
/// sent once it answers, `None` where the connection ended without an answer head.
fn reset_later(address: SocketAddr) -> mpsc::Receiver<Option<u16>> {
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let status = (|| {
            let mut stream = TcpStream::connect_timeout(&address, ANSWER).ok()?;
            stream.set_read_timeout(Some(ANSWER)).ok()?;
            write!(
                stream,
                "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: 0\r\n\r\n"
            )
            .ok()?;
            let mut answer = Vec::new();
            let _ = stream.read_to_end(&mut answer);
            String::from_utf8_lossy(&answer)
                .split(' ')
                .nth(1)
                .and_then(|status| status.parse().ok())
        })();
        let _ = answered.send(status);
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

/// The exit status of `served` once it exits by itself, within [`ANSWER`].
fn exited(served: &mut Served) -> ExitStatus {
    let deadline = Instant::now() + ANSWER;
    loop {
        if let Some(status) = served.child.try_wait().expect("the run is polled") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the run did not exit within {ANSWER:?}: {:#?}",
            served.seen
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The events of `name` the run at `served` published.
fn of(served: &Served, name: &str) -> usize {
    published(&served.events(&[]))
        .iter()
        .filter(|(event, _)| event == name)
        .count()
}

/// How many entries `seed.json` in `state` records done.
fn done(state: &str) -> Value {
    let text = std::fs::read_to_string(Path::new(state).join("seed.json"))
        .expect("the seed marker is read");
    serde_json::from_str::<Value>(&text).expect("the marker is JSON")["done"].clone()
}

/// The start's seed ran to its end; a reset is taken and its seed's first entry is held when
/// SIGTERM arrives. Once that entry is done the reset answers `200` and the run exits 143; a
/// restart on the state without `--seed` holds what that entry applied.
#[test]
fn a_reset_taken_after_a_signal_is_answered_200() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = super::write(
        &directory,
        "seed.json",
        &json!([vouch("v-1", "seed-1"), grant("admin", "north", "seed-2")]).to_string(),
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
    release.send(()).expect("the desk is released");
    served.ended();
    let answer = reset_later(served.address);
    asked.recv_timeout(ANSWER).expect("the reset's seed asks");
    terminate(&served);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    assert_eq!(
        answer.recv_timeout(ANSWER).expect("the client reads"),
        Some(200),
        "the reset taken before the signal is answered once its entry under way is done"
    );
    assert_eq!(exited(&mut served).code(), Some(143));
    served.stop();

    let restarted = serve(&["--environment", &document, "--state", state]);
    assert_eq!(of(&restarted, "vault.access.Vouched"), 1);
    assert!(role_ids(&restarted).is_empty(), "the grant was never sent");
}

/// The start's seed is held at its second entry and a reset is queued when SIGTERM arrives: the
/// loop does not take it, the client reads `503`, and a restart on the state without `--seed`
/// holds what the start's seed applied, so the reset did not empty it.
#[test]
fn a_request_queued_at_the_signal_reads_503() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = super::write(
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
    asked.recv_timeout(ANSWER).expect("the second entry asks");
    let answer = reset_later(served.address);
    // The reset reaches the loop's queue while the loop waits for the desk.
    std::thread::sleep(Duration::from_millis(300));
    terminate(&served);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    assert_eq!(
        answer.recv_timeout(ANSWER).expect("the client reads"),
        Some(503),
        "a request queued at the signal is answered 503, never a closed connection"
    );
    assert_eq!(exited(&mut served).code(), Some(143));
    served.stop();

    let restarted = serve(&["--environment", &document, "--state", state]);
    assert_eq!(role_ids(&restarted), ["admin"]);
    assert_eq!(of(&restarted, "vault.access.Vouched"), 1);
}

/// On a state whose seed ran to its end, a reset whose first seed entry is held is cut by SIGTERM:
/// the entry finishes, the reset answers `200`, and a restart with the same seed sends it from its
/// second entry, since the reset recorded 0 entries done before its first.
#[test]
fn a_reset_cut_at_its_first_entry_resumes_from_the_second() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = super::write(
        &directory,
        "seed.json",
        &json!([
            vouch("v-1", "seed-1"),
            grant("admin", "north", "seed-2"),
            grant("auditor", "south", "seed-3"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "sqlite", Some(desk), "");
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
    ];
    let mut served = serve(&arguments);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    release.send(()).expect("the desk is released");
    let ended = served.ended();
    assert!(ended.contains("3 sent, 3 applied"), "{ended}");
    let answer = reset_later(served.address);
    asked.recv_timeout(ANSWER).expect("the reset's seed asks");
    terminate(&served);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    assert_eq!(
        answer.recv_timeout(ANSWER).expect("the client reads"),
        Some(200)
    );
    assert_eq!(exited(&mut served).code(), Some(143));
    assert_eq!(done(state), 1, "the old full count is gone");
    served.stop();

    let mut restarted = serve(&arguments);
    let ended = restarted.ended();
    assert!(
        ended.contains("2 sent, 2 applied, 0 already there, 0 refused"),
        "the second and third entries: {ended}"
    );
    assert_eq!(role_ids(&restarted), ["admin", "auditor"]);
    assert_eq!(of(&restarted, "vault.access.Vouched"), 1);
    assert!(asked.try_recv().is_err(), "the desk was asked again");
}
