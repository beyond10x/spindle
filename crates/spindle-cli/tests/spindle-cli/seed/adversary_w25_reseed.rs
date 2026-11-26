//! Adversary cases of wave 25 against `story:reset-reapplies-seed`: a reset on a seeded run.

use std::time::{Duration, Instant};

use serde_json::json;

use super::{
    ANSWER, VISITOR, VOUCH, counted, environment, grant, guest_world, held_desk, holding_desk,
    reset, role_ids, scratch, serve, write,
};

/// SIGTERM while a reset's seed run is held at its first entry: the run exits 143 once that entry
/// is done, before it submits the next, as it does between two entries of the start's seed and
/// between two requests. The reset's seed runs every entry in one call
/// (`Seeding::finish`), with no look at the signal between them.
#[test]
fn sigterm_during_a_resets_seed_stops_the_run_between_entries() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let vouch = |voucher: &str, request: &str| {
        json!({
            "command": VOUCH,
            "input": {"voucher_id": voucher, "role_id": "admin"},
            "actor": VISITOR,
            "request_id": request,
        })
    };
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
        asked
            .recv_timeout(ANSWER)
            .expect("the start's seed asks the desk");
        release.send(()).expect("the desk is released");
    }
    served.ended();

    let address = served.address;
    std::thread::spawn(move || {
        let _ = reset(address, None);
    });
    asked
        .recv_timeout(ANSWER)
        .expect("the reset's seed asks the desk for its second entry");
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("SIGTERM is sent");
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");

    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = served.child.try_wait().expect("waited on") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let asked_again = asked.try_recv().is_ok();
    assert!(
        status.is_some() && !asked_again,
        "SIGTERM during a reset's seed: exited {status:?}, the desk asked again for the next \
         entry: {asked_again}; the run went on submitting the seed after the signal"
    );
    assert_eq!(status.and_then(|status| status.code()), Some(143));
}

/// A reset whose body gives a world, arriving while the start's seed is held at its second entry:
/// the docs this story wrote say such a reset "loads that world alone and does not submit the
/// seed", but the start's entries left after it are submitted into the world once it answers.
#[test]
fn a_reset_with_a_world_during_the_start_seed_loads_that_world_alone() {
    let directory = scratch();
    let (desk, asked, release) = held_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {
                "command": VOUCH,
                "input": {"voucher_id": "v-1", "role_id": "admin"},
                "actor": VISITOR,
                "request_id": "seed-2",
            },
            grant("auditor", "south", "seed-3"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed]);
    asked
        .recv_timeout(ANSWER)
        .expect("the start's seed asks the desk for its second entry");
    let address = served.address;
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, Some(&guest_world())));
    });
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).expect("the desk is released");
    let (status, body) = answer.recv_timeout(ANSWER).expect("the reset answers");
    assert_eq!(status, 200, "{body}");
    served.ended();
    assert_eq!(
        role_ids(&served),
        ["guest"],
        "a reset with a world loads that world alone; the start's seed went on into it"
    );
}

/// README.md:99, `run --observe --help` (`cli.rs:280`), `spindle-ingress` (`lib.rs:66`) and the
/// metrics module (`observe/metrics.rs:18`, "no count falls") say a reset keeps every count, and
/// `spindle_seed_commands_total` is a counter. A scrape while a reset's seed run is held at its
/// second entry reads counts below those the start's seed left.
#[test]
fn no_seed_count_falls_across_a_reset() {
    let directory = scratch();
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {
                "command": VOUCH,
                "input": {"voucher_id": "v-1", "role_id": "admin"},
                "actor": VISITOR,
                "request_id": "seed-2",
            },
            grant("auditor", "south", "seed-3"),
        ])
        .to_string(),
    );
    let document = environment(&directory, "memory", Some(desk), "");
    let mut served = serve(&["--environment", &document, "--seed", &seed, "--observe"]);
    asked
        .recv_timeout(ANSWER)
        .expect("the start's seed asks the desk");
    release.send(()).expect("the desk is released");
    served.ended();
    let before = counted(served.address);
    assert_eq!(before, [3, 3, 0, 0]);

    let address = served.address;
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, None));
    });
    asked
        .recv_timeout(ANSWER)
        .expect("the reset's seed asks the desk");
    let during = counted(served.address);
    release.send(()).expect("the desk is released");
    let _ = answer.recv_timeout(ANSWER).expect("the reset answers");
    assert!(
        during
            .iter()
            .zip(before)
            .all(|(during, before)| *during >= before),
        "a counter fell across a reset: {before:?} before it, {during:?} during its seed"
    );
}
