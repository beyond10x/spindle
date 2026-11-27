//! Adversary of wave 27 U2 (`story:seed-once-on-durable-state`,
//! `story:serve-loop-signal-exit-answers`): the seed marker read against a rejected entry, a
//! request id a resumed seed repeats and a reset made by a run without a seed; and the serve loop's
//! signal exit with a request that completes after the signal, with SIGINT, and with a command
//! under way.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, GRANT, OPERATOR, Served, VISITOR, VOUCH, environment, grant, holding_desk, published,
    reset, role_ids, route, scratch, serve, write,
};

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

/// Sends `signal` to the run.
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
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// How many entries `seed.json` in `state` records done.
fn done(state: &str) -> Value {
    let text = std::fs::read_to_string(Path::new(state).join("seed.json"))
        .expect("the seed marker is read");
    serde_json::from_str::<Value>(&text).expect("the marker is JSON")["done"].clone()
}

/// The events of `name` the run at `served` published.
fn of(served: &Served, name: &str) -> usize {
    published(&served.events(&[]))
        .iter()
        .filter(|(event, _)| event == name)
        .count()
}

/// The status of one HTTP/1.1 `method` on `path` with `body`, on its own connection from a thread
/// of its own, sent once it answers; `None` where the connection ended without an answer head.
fn later(
    address: SocketAddr,
    method: String,
    path: String,
    body: String,
) -> mpsc::Receiver<Option<u16>> {
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
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
        let _ = answered.send(status);
    });
    answer
}

/// `story:run-fails-on-rejected-line`: a seed that ends having rejected an entry stops the run with
/// exit 1. On a `--state` the marker counts the rejected entry done, so a restart with the same,
/// unchanged seed prints that the seed already ran and serves: the entry that did not run is never
/// sent again and the run no longer exits 1. Before the marker, the restart sent it again and
/// exited 1 again.
#[test]
fn a_restart_after_a_seed_that_rejected_an_entry_does_not_hide_the_rejection() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-admin"),
            {"command": GRANT, "input": {"role_id": "spy", "title": "Spy"}, "actor": VISITOR,
             "request_id": "seed-visitor-grant"},
        ])
        .to_string(),
    );
    let document = environment(&directory, "sqlite", None, "");
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--answer-default",
    ];
    let mut first = serve(&arguments);
    let ended = first.ended();
    assert!(ended.contains("1 rejected"), "{ended}");
    let status = exited_within(&mut first, ANSWER).expect("the rejecting seed stops the run");
    assert_eq!(status.code(), Some(1));
    first.stop();

    let mut second = serve(&arguments);
    let line = second.line("b10x-spindle: seed ");
    let status = exited_within(&mut second, Duration::from_secs(5));
    let stderr = second.stop();
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(1),
        "a restart with the seed whose entry was rejected sends it again and exits 1 again; it \
         printed {line:?} and kept serving: {stderr:#?}"
    );
}

/// A seed whose third entry repeats the second's request id: in one run the repeat counts as
/// already there and is not submitted (`seed` module doc). Stopped by SIGTERM while the second is
/// held, the run records 2 done; the restart resumes at the third, has forgotten that `seed-2` was
/// applied, and submits the repeat, so the desk is asked again and a second voucher is attempted.
#[test]
fn a_resumed_seed_does_not_submit_a_request_id_applied_before_the_stop() {
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
            vouch("v-1", "seed-2"),
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
    let mut first = serve(&arguments);
    asked
        .recv_timeout(ANSWER)
        .expect("the second entry asks the desk");
    signal(&first, rustix::process::Signal::TERM);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    let status = exited_within(&mut first, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143));
    assert_eq!(done(state), 2);
    first.stop();

    let mut second = serve(&arguments);
    let asked_again = asked.recv_timeout(Duration::from_secs(3)).is_ok();
    if asked_again {
        let _ = release.send(());
    }
    let ended = second.ended();
    assert!(
        !asked_again && ended.contains("2 sent, 1 applied, 1 already there, 0 refused, 0 rejected"),
        "the repeat of seed-2, applied before the stop, is already there and the desk is not asked \
         again (asked again: {asked_again}): {ended}"
    );
}

/// A run with `--seed` on a state, then a run on that state without `--seed` whose reset with no
/// world empties it: `seed.json` still reads every entry done. A third start with the same `--seed`
/// on the empty state prints that the seed already ran and sends nothing, where before the marker
/// it seeded the empty state.
#[test]
fn a_seed_after_a_reset_by_a_run_without_it_applies_to_the_emptied_state() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "sqlite", None, "");
    let seeded = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--answer-default",
    ];
    let mut first = serve(&seeded);
    first.ended();
    assert_eq!(role_ids(&first), ["admin"]);
    first.stop();

    let second = serve(&[
        "--environment",
        &document,
        "--state",
        state,
        "--answer-default",
    ]);
    let (status, body) = reset(second.address, None);
    assert_eq!(status, 200, "{body}");
    assert!(role_ids(&second).is_empty(), "the reset emptied the state");
    second.stop();

    let mut third = serve(&seeded);
    let line = third.line("b10x-spindle: seed ");
    assert!(
        !line.contains("already ran"),
        "the state holds none of the seed's rows, yet the start skips the seed: {line}"
    );
    third.ended();
    assert_eq!(role_ids(&third), ["admin"]);
}

/// A reset whose head reached the listener before SIGTERM and whose last bytes arrive right after
/// it, while the loop waits for an exchange: the loop does not take it, the client reads `503`, the
/// run exits 143 and the state keeps the seed's row. Kills the mutant that drops the signal check
/// on an exchange `listener.next` returns.
#[test]
fn a_reset_completed_after_the_signal_reads_503_and_leaves_the_state() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "sqlite", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--answer-default",
    ]);
    served.ended();
    let address = served.address;
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n"
    )
    .expect("the head is written");
    std::thread::sleep(Duration::from_millis(200));
    signal(&served, rustix::process::Signal::TERM);
    write!(stream, "Content-Length: 0\r\n\r\n").expect("the head is finished");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
    assert!(
        answer.starts_with("HTTP/1.1 503"),
        "a reset completed after the signal reads 503: {answer:?}"
    );
    let status = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(status.code(), Some(143));
    served.stop();

    let restarted = serve(&[
        "--environment",
        &document,
        "--state",
        state,
        "--answer-default",
    ]);
    assert_eq!(role_ids(&restarted), ["admin"], "the reset was not taken");
}

/// SIGINT while the start's seed holds its second entry and a grant is queued, on a run given
/// `--drain 0`: the grant reads `503`, the run exits 130, and the state holds what the seed
/// applied, never the grant. With the default bound the queued grant is answered instead
/// (`story:health-and-graceful-drain`, D4).
#[test]
fn sigint_with_a_grant_queued_reads_503_and_exits_130() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1"), vouch("v-1", "seed-2")]).to_string(),
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
        "--drain",
        "0",
    ]);
    asked.recv_timeout(ANSWER).expect("the second entry asks");
    let (method, path) = route(GRANT);
    let answer = later(
        served.address,
        method,
        path,
        json!({"role_id": "queued", "title": "Queued"}).to_string(),
    );
    std::thread::sleep(Duration::from_millis(300));
    signal(&served, rustix::process::Signal::INT);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    assert_eq!(
        answer.recv_timeout(ANSWER).expect("the client reads"),
        Some(503)
    );
    let status = exited_within(&mut served, ANSWER).expect("the run exits on SIGINT");
    assert_eq!(status.code(), Some(130));
    assert_eq!(done(state), 2);
    served.stop();

    let restarted = serve(&["--environment", &document, "--state", state]);
    assert_eq!(role_ids(&restarted), ["admin"]);
    assert_eq!(of(&restarted, "vault.access.Vouched"), 1);
}

/// A command sent over HTTP is under way, held at the desk, when SIGTERM arrives: once the desk
/// answers, the client reads the command's 2xx answer and the run exits 143.
#[test]
fn a_command_under_way_at_the_signal_is_answered() {
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
    let (method, path) = route(VOUCH);
    let answer = later(
        served.address,
        method,
        path,
        json!({"voucher_id": "v-9", "role_id": "admin"}).to_string(),
    );
    asked
        .recv_timeout(ANSWER)
        .expect("the command asks the desk");
    signal(&served, rustix::process::Signal::TERM);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    let status = answer.recv_timeout(ANSWER).expect("the client reads");
    assert!(
        status.is_some_and(|status| (200..300).contains(&status)),
        "the command under way at the signal is answered: {status:?}"
    );
    let exit = exited_within(&mut served, ANSWER).expect("the run exits on SIGTERM");
    assert_eq!(exit.code(), Some(143));
}
