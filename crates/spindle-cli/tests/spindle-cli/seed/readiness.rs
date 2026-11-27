//! A served run answers over HTTP whether its seed is running or has ended, with its counts
//! (`story:seed-readiness`): `GET /__spindle/seed` answers `{"state": "none" | "running" |
//! "ended", "entries", "left", "sent", "applied", "already_there", "refused", "rejected"}`, and
//! `GET /__spindle/ready` answers `503` while the seed runs and `200` once it has ended, both
//! without `--observe` and without the host's thread.

use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, HEADER, RETIRE, ROLES, SECRET, SECRET_HEADER, VISITOR, VOUCH, environment, exchange,
    grant, held_desk, holding_desk, reset, route, scratch, sent, serve, write,
};

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

/// `GET <path>` on the run at `address` with `headers`: the status and the JSON body.
fn read(address: SocketAddr, path: &str, headers: &[(&str, &str)]) -> (u16, Value) {
    let (status, body) = exchange(address, "GET", path, headers);
    let body = serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("{path} answers JSON ({error}): {body:?}"));
    (status, body)
}

/// `GET /__spindle/seed`, which answers `200`.
fn seed_state(address: SocketAddr) -> Value {
    let (status, body) = read(address, "/__spindle/seed", &[]);
    assert_eq!(status, 200, "{body}");
    body
}

/// The status `GET /__spindle/ready` answers.
fn readiness(address: SocketAddr) -> u16 {
    read(address, "/__spindle/ready", &[]).0
}

/// The counts the seed's end line prints: `sent`, `applied`, `already there`, `refused`,
/// `rejected`.
fn end_counts(line: &str) -> [u64; 5] {
    ["sent", "applied", "already there", "refused", "rejected"].map(|counted| {
        let (before, _) = line
            .split_once(&format!(" {counted}"))
            .unwrap_or_else(|| panic!("the end line counts {counted:?}: {line}"));
        before
            .rsplit([' ', ','])
            .find(|word| !word.is_empty())
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("a count before {counted:?}: {line}"))
    })
}

/// The counts `GET /__spindle/seed` answered, in the end line's order.
fn answered_counts(state: &Value) -> [u64; 5] {
    ["sent", "applied", "already_there", "refused", "rejected"].map(|counted| {
        state[counted]
            .as_u64()
            .unwrap_or_else(|| panic!("the state counts {counted:?}: {state}"))
    })
}

/// While the desk holds the second of three entries the seed reads `running` with two entries
/// left and the first applied; once it ended it reads `ended` with the end line's counts.
#[test]
fn the_seed_state_is_answered_while_it_runs_and_once_it_ends() {
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
    asked
        .recv_timeout(ANSWER)
        .expect("the seed's second entry asks the desk");
    assert_eq!(
        seed_state(served.address),
        json!({"state": "running", "entries": 3, "left": 2, "sent": 1, "applied": 1,
               "already_there": 0, "refused": 0, "rejected": 0}),
        "the first entry is done, the second held"
    );
    release.send(()).expect("the desk is released");
    let ended = served.ended();
    let state = seed_state(served.address);
    assert_eq!(state["state"], "ended", "{state}");
    assert_eq!(state["entries"], 3, "{state}");
    assert_eq!(state["left"], 0, "{state}");
    assert_eq!(
        answered_counts(&state),
        end_counts(&ended),
        "{state} against {ended}"
    );
    assert_eq!(answered_counts(&state), [3, 3, 0, 0, 0]);
}

/// `GET /__spindle/ready` answers `503` while an entry is left and `200` once the seed ended, a
/// declared refusal among its entries; a run without a seed answers `200` from its first request,
/// and its seed reads `none`.
#[test]
fn a_readiness_route_answers_200_only_after_the_seed_ends() {
    let directory = scratch();
    let (desk, asked, release) = held_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            vouch("v-1", "seed-2"),
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": super::OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-3"},
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed]);
    asked
        .recv_timeout(ANSWER)
        .expect("the seed's second entry asks the desk");
    let (status, body) = read(served.address, "/__spindle/ready", &[]);
    assert_eq!(status, 503, "an entry is left: {body}");
    release.send(()).expect("the desk is released");
    let ended = served.ended();
    assert!(ended.contains("1 refused"), "{ended}");
    let (status, body) = read(served.address, "/__spindle/ready", &[]);
    assert_eq!(
        status, 200,
        "a declared refusal leaves the run ready: {body}"
    );
    served.stop();

    let unseeded = environment(&directory, "memory", None, "");
    let served = serve(&["--environment", &unseeded, "--answer-default"]);
    let (status, body) = read(served.address, "/__spindle/ready", &[]);
    assert_eq!(status, 200, "a run without a seed is ready at once: {body}");
    assert_eq!(seed_state(served.address), json!({"state": "none"}));
}

/// Both routes answer on a run without `--observe`, whose other `/__spindle/` paths answer `404`.
#[test]
fn the_seed_state_needs_no_observe() {
    let directory = scratch();
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let document = environment(&directory, "memory", None, "");
    let mut served = serve(&[
        "--environment",
        &document,
        "--seed",
        &seed,
        "--answer-default",
    ]);
    served.ended();
    let (status, _) = exchange(served.address, "GET", "/__spindle/requests", &[]);
    assert_eq!(status, 404, "the run does not observe");
    assert_eq!(seed_state(served.address)["state"], "ended");
    assert_eq!(readiness(served.address), 200);
}

/// On a `--trusted-caller-header` run both routes answer `403` without the proxy's secret, as the
/// console routes do, and as on any run with it.
#[test]
fn a_trusted_caller_run_guards_the_readiness_route_as_its_console() {
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
    for path in ["/__spindle/seed", "/__spindle/ready"] {
        let (status, body) = exchange(served.address, "GET", path, &[]);
        assert_eq!(status, 403, "{path} without the secret: {body}");
    }
    let (status, state) = read(
        served.address,
        "/__spindle/seed",
        &[(SECRET_HEADER, SECRET)],
    );
    assert_eq!(status, 200, "{state}");
    assert_eq!(state["state"], "ended", "{state}");
    let (status, body) = read(
        served.address,
        "/__spindle/ready",
        &[(SECRET_HEADER, SECRET)],
    );
    assert_eq!(status, 200, "{body}");
}

/// During a reset's seed, held at its second entry, the seed reads `running` with that run's
/// counts and the run is not ready; once it ends, `ended` with that run's counts and `200`.
#[test]
fn a_reset_that_seeds_again_answers_running_until_it_ends() {
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
    let mut served = serve(&["--environment", &document, "--seed", &seed]);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    release.send(()).expect("the desk is released");
    served.ended();
    assert_eq!(readiness(served.address), 200);

    let address = served.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, None));
    });
    asked.recv_timeout(ANSWER).expect("the reset's seed asks");
    assert_eq!(
        seed_state(served.address),
        json!({"state": "running", "entries": 3, "left": 2, "sent": 1, "applied": 1,
               "already_there": 0, "refused": 0, "rejected": 0}),
        "the reset's seed run alone"
    );
    assert_eq!(readiness(served.address), 503);
    release.send(()).expect("the desk is released");
    let (status, body) = answer.recv_timeout(ANSWER).expect("the reset answers");
    assert_eq!(status, 200, "{body}");
    let state = seed_state(served.address);
    assert_eq!(state["state"], "ended", "{state}");
    assert_eq!(answered_counts(&state), [3, 3, 0, 0, 0], "{state}");
    assert_eq!(readiness(served.address), 200);
}

/// On a `--state` whose marker records every entry done, the first request reads `ended`, no
/// entry sent by this start, and the run is ready.
#[test]
fn a_seed_already_run_on_a_kept_state_is_ended_from_the_first_request() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            grant("auditor", "north", "seed-2")
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
    first.ended();
    first.stop();

    let second = serve(&arguments);
    assert_eq!(
        seed_state(second.address),
        json!({"state": "ended", "entries": 2, "left": 0, "sent": 0, "applied": 0,
               "already_there": 0, "refused": 0, "rejected": 0}),
    );
    assert_eq!(readiness(second.address), 200);
}

/// The first `200` on `/__spindle/ready` is followed at once by a read of the view, which holds
/// the row the seed's last entry wrote: the seed reads `ended` only once that entry's write is in
/// place.
#[test]
fn a_view_holds_the_last_entry_once_ready_reads_200() {
    const LAST: usize = 20;
    let directory = scratch();
    let (desk, asked, release) = held_desk();
    let mut entries = vec![grant("r-0", "north", "seed-0"), vouch("v-1", "seed-v")];
    entries.extend((1..=LAST).map(|n| grant(&format!("r-{n}"), "north", &format!("seed-{n}"))));
    let seed = write(&directory, "seed.json", &json!(entries).to_string());
    let document = environment(&directory, "memory", Some(desk), "");
    let served = serve(&["--environment", &document, "--seed", &seed]);
    let (method, path) = route(ROLES);
    asked
        .recv_timeout(ANSWER)
        .expect("the seed's second entry asks the desk");
    assert_eq!(readiness(served.address), 503);
    release.send(()).expect("the desk is released");
    let deadline = Instant::now() + ANSWER;
    loop {
        assert!(
            Instant::now() < deadline,
            "the run was not ready within {ANSWER:?}"
        );
        if readiness(served.address) == 200 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let (status, body) = sent(served.address, &method, &path, "");
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("the view answers JSON");
    let last = format!("r-{LAST}");
    assert!(
        body["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["role_id"] == last.as_str())),
        "the view holds the last entry's row once ready reads 200: {body}"
    );
}
