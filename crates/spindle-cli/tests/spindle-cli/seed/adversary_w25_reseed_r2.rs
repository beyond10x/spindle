//! Adversary cases of wave 25, round 2, against `story:reset-reapplies-seed`: the cut of a seed
//! run under way, the counter summed over seed runs, and a signal around a reset's seed.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitStatus;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, OPERATOR, RETIRE, ROLE, Served, VISITOR, VOUCH, counted, environment, grant,
    guest_world, held_desk, holding_desk, published, reset, resets, role_ids, scratch, serve,
    write,
};

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

/// `POST /api/reset` with `body`, on its own connection: the status, or `None` where the
/// connection ended without an answer head.
fn reset_status(address: SocketAddr, body: &str) -> Option<u16> {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).ok()?;
    stream.set_read_timeout(Some(ANSWER)).ok()?;
    write!(
        stream,
        "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .ok()?;
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer);
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
}

fn signal(served: &Served, signal: rustix::process::Signal) {
    rustix::process::kill_process(rustix::process::Pid::from_child(&served.child), signal)
        .expect("the signal is sent");
}

/// The exit status of `served` once it exits within `within`.
fn exited(served: &mut Served, within: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(status) = served.child.try_wait().expect("waited on") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// SIGINT during a reset's seed, as SIGTERM in round 1: the run exits 130 once the entry under
/// way is done, and the desk is not asked for the next one. The reset is answered `200` before the
/// run exits (`story:serve-loop-signal-exit-answers`, D2).
#[test]
fn sigint_during_a_resets_seed_exits_130_between_entries() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            vouch("v-1", "seed-2"),
            vouch("v-2", "seed-3"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed]);
    for _ in 0..2 {
        asked.recv_timeout(ANSWER).expect("the start's seed asks");
        release.send(()).expect("released");
    }
    served.ended();
    let address = served.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset_status(address, ""));
    });
    asked
        .recv_timeout(ANSWER)
        .expect("the reset's seed asks the desk for its second entry");
    signal(&served, rustix::process::Signal::INT);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("released");
    let status = exited(&mut served, Duration::from_secs(3));
    assert!(
        asked.try_recv().is_err(),
        "the desk was asked for the next entry after SIGINT"
    );
    assert_eq!(status.and_then(|status| status.code()), Some(130));
    assert_eq!(
        answer.recv_timeout(ANSWER).expect("the client reads"),
        Some(200),
        "the reset cut by the signal is answered once the entry under way is done"
    );
}

/// A reset with no world while the start's seed is held at its second entry: the start's run is
/// cut, its end line counting the two entries it sent, and the reset's run sends all three; the
/// counter is the sum of both runs.
#[test]
fn a_reset_with_no_world_during_the_start_seed_cuts_it_and_runs_the_seed_whole() {
    let directory = scratch();
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
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed, "--observe"]);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    let address = served.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, None));
    });
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).expect("released");
    asked
        .recv_timeout(ANSWER)
        .expect("the reset's seed asks the desk again");
    release.send(()).expect("released");
    let (status, body) = answer.recv_timeout(ANSWER).expect("the reset answers");
    assert_eq!(status, 200, "{body}");
    let first = served.ended_times(1);
    assert!(
        first.contains("2 sent, 2 applied, 0 already there, 0 refused"),
        "{first}"
    );
    let second = served.ended_times(2);
    assert!(
        second.contains("3 sent, 3 applied, 0 already there, 0 refused"),
        "{second}"
    );
    served.line("a reset dropped the 1 entries not yet sent");
    assert_eq!(counted(served.address), [5, 5, 0, 0]);
    assert_eq!(role_ids(&served), ["admin", "auditor"]);
    assert_eq!(published(&served.events(&[])).len(), 3);
    assert_eq!(resets(served.address), 1);
}

/// Four resets, one with a world, on a seed with an applied, a refused and an already-there entry:
/// the counter sums every run and never falls; a world reset after the seed ended drops nothing,
/// prints no end line and adds nothing; each end line counts its run alone.
#[test]
fn the_seed_counter_sums_every_run_across_resets_with_a_world_between() {
    let directory = scratch();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
            grant("admin", "north", "seed-1"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
        "--observe",
    ]);
    let run = "3 sent, 1 applied, 1 already there, 1 refused";
    assert!(served.ended().contains(run));
    assert_eq!(counted(served.address), [3, 1, 1, 1]);

    assert_eq!(reset(served.address, None).0, 200);
    assert!(served.ended_times(2).contains(run));
    assert_eq!(counted(served.address), [6, 2, 2, 2]);

    assert_eq!(reset(served.address, Some(&guest_world())).0, 200);
    assert_eq!(role_ids(&served), ["guest"]);
    assert_eq!(counted(served.address), [6, 2, 2, 2]);

    assert_eq!(reset(served.address, None).0, 200);
    assert!(served.ended_times(3).contains(run));
    assert_eq!(counted(served.address), [9, 3, 3, 3]);
    assert_eq!(role_ids(&served), ["admin"]);

    assert_eq!(reset(served.address, None).0, 200);
    assert!(served.ended_times(4).contains(run));
    assert_eq!(counted(served.address), [12, 4, 4, 4]);
    assert_eq!(resets(served.address), 4);

    let stderr = served.stop();
    let ends = stderr.iter().filter(|l| l.contains(" ended: ")).count();
    assert_eq!(ends, 4, "{stderr:#?}");
    assert!(
        stderr.iter().all(|l| !l.contains("dropped")),
        "no reset came while a run was under way: {stderr:#?}"
    );
}

/// A world reset cuts the start's run; a later reset with no world runs the seed whole, its end
/// line counting it alone, and the counter adds it to what the cut run counted.
#[test]
fn a_world_reset_cuts_the_start_and_a_later_plain_reset_adds_to_the_cut_counts() {
    let directory = scratch();
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
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed, "--observe"]);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    let address = served.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, Some(&guest_world())));
    });
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).expect("released");
    assert_eq!(answer.recv_timeout(ANSWER).expect("answers").0, 200);
    assert!(served.ended_times(1).contains("2 sent, 2 applied"));
    assert_eq!(role_ids(&served), ["guest"]);
    assert_eq!(counted(served.address), [2, 2, 0, 0]);

    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, None));
    });
    asked.recv_timeout(ANSWER).expect("the reset's seed asks");
    release.send(()).expect("released");
    assert_eq!(answer.recv_timeout(ANSWER).expect("answers").0, 200);
    assert!(
        served
            .ended_times(2)
            .contains("3 sent, 3 applied, 0 already there, 0 refused")
    );
    assert_eq!(counted(served.address), [5, 5, 0, 0]);
    assert_eq!(role_ids(&served), ["admin", "auditor"]);
}

/// A world reset refused `400` while the start's seed is held: the seed goes on to its end, no
/// entry dropped, no reset counted.
#[test]
fn a_refused_world_reset_leaves_the_start_seed_running() {
    let directory = scratch();
    let (desk, asked, release) = held_desk();
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
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed, "--observe"]);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    let row = json!({"entity": ROLE, "identity": "guest", "state": "Active",
                     "fields": {"title": "Guest", "tenant": "east"}});
    let twice = json!({"rows": [row, row]});
    let address = served.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, Some(&twice)));
    });
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).expect("released");
    assert_eq!(answer.recv_timeout(ANSWER).expect("answers").0, 400);
    assert!(
        served
            .ended()
            .contains("3 sent, 3 applied, 0 already there, 0 refused")
    );
    assert_eq!(role_ids(&served), ["admin", "auditor"]);
    assert_eq!(resets(served.address), 0);
    let stderr = served.stop();
    assert!(stderr.iter().all(|l| !l.contains("dropped")), "{stderr:#?}");
}

/// An empty seed: a reset with no world prints its end line again, `0 sent`.
#[test]
fn an_empty_seed_resets_and_ends_again() {
    let directory = scratch();
    let seed = write(&directory, "seed.json", "[]");
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
        "--observe",
    ]);
    assert!(served.ended().contains("0 sent"));
    assert_eq!(reset(served.address, None).0, 200);
    assert!(served.ended_times(2).contains("0 sent, 0 applied"));
    assert_eq!(counted(served.address), [0, 0, 0, 0]);
}
