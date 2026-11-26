//! A seed is applied once to a durable state (`story:seed-once-on-durable-state`): a run on a
//! `--state` records in `seed.json`, beside the state's files, the digest of the seed file and how
//! many of its entries are done, after each entry. A restart with the same file sends only the
//! entries not yet done, and nothing once all are; one with another file stops before listening,
//! naming both digests, unless the state holds no row. A reset with no world writes the count 0
//! before it sends the seed again; one that loads a world marks every entry done.

use std::path::Path;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    ANSWER, OPERATOR, RETIRE, SPEC, Served, VISITOR, VOUCH, counted, environment, grant,
    guest_world, holding_desk, published, reset, role_ids, roles, scratch, serve, start, write,
};

/// The file a `--state` records its seed in.
const MARKER: &str = "seed.json";

fn vouch(voucher: &str, request: &str) -> Value {
    json!({
        "command": VOUCH,
        "input": {"voucher_id": voucher, "role_id": "admin"},
        "actor": VISITOR,
        "request_id": request,
    })
}

/// The digest `run` records of the seed file at `path`.
fn digest(path: &str) -> String {
    ess_conformance::known_failures::sha256(&std::fs::read(path).expect("the seed file is read"))
}

/// What the state at `state` records of its seed.
fn marker(state: &str) -> Value {
    let text = std::fs::read_to_string(Path::new(state).join(MARKER))
        .unwrap_or_else(|error| panic!("{state}/{MARKER} is read: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"))
}

/// Sends `signal` to the run.
fn signal(served: &Served, signal: rustix::process::Signal) {
    rustix::process::kill_process(rustix::process::Pid::from_child(&served.child), signal)
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

/// A seed that grants a role and then retires it, run on a state, stopped and run again: the second
/// start prints that the seed already ran and sends nothing, so the role was retired once.
#[test]
fn a_restart_does_not_apply_the_seed_again() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {"command": RETIRE, "input": {"role_id": "admin"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-2"},
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
        "--observe",
    ];
    let mut first = serve(&arguments);
    let ended = first.ended();
    assert!(ended.contains("2 sent, 2 applied"), "{ended}");
    first.stop();

    let mut second = serve(&arguments);
    let skipped = second.line("already ran");
    assert!(
        skipped.contains(&seed) && skipped.contains(&digest(&seed)),
        "the line names the file and its digest: {skipped}"
    );
    assert_eq!(counted(second.address), [0, 0, 0, 0]);
    assert_eq!(of(&second, "vault.access.RoleRetired"), 1);
    let rows = roles(&second);
    assert_eq!(rows.len(), 1, "{rows:#?}");
    assert_eq!(rows[0]["state"], "Retired", "{rows:#?}");
    let stderr = second.stop();
    assert!(
        stderr.iter().all(|line| !line.contains(" ended: ")),
        "no seed run started: {stderr:#?}"
    );
}

/// A restart with an edited seed file on a state that holds rows exits 1 before listening, naming
/// the digest the state recorded and the digest of the file given.
#[test]
fn a_changed_seed_on_a_seeded_state_stops_the_run() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
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
    let recorded = digest(&seed);

    write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            grant("auditor", "north", "seed-2"),
        ])
        .to_string(),
    );
    let given = digest(&seed);
    assert_ne!(recorded, given);
    let Err((code, stdout, stderr)) = start(SPEC, &arguments) else {
        panic!("the run listened with a seed the state was not seeded with");
    };
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stdout.is_empty(), "no listening line: {stdout}");
    assert!(
        stderr.contains(&recorded) && stderr.contains(&given) && stderr.contains(&seed),
        "both digests and the file are named: {stderr}"
    );
    assert_eq!(marker(state)["digest"], json!(recorded));
}

/// After a start that skipped the seed, a reset with no world empties the state and applies the
/// seed from its first entry before it answers.
#[test]
fn a_reset_applies_the_seed_again_after_a_restart() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            grant("auditor", "south", "seed-2"),
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
        "--observe",
    ];
    let mut first = serve(&arguments);
    first.ended();
    first.stop();

    let mut second = serve(&arguments);
    second.line("already ran");
    let (status, body) = reset(second.address, None);
    assert_eq!(status, 200, "{body}");
    let ended = second.ended();
    assert!(
        ended.contains("2 sent, 2 applied, 0 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(role_ids(&second), ["admin", "auditor"]);
    assert_eq!(counted(second.address), [2, 2, 0, 0]);
    assert_eq!(marker(state)["done"], 2);
}

/// A reset that loads a world marks every seed entry done: a restart with the same seed sends none,
/// and the state holds the world's rows alone.
#[test]
fn a_restart_after_a_reset_with_a_world_sends_no_seed_entry() {
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
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--observe",
    ];
    let first = serve(&arguments);
    asked.recv_timeout(ANSWER).expect("the seed asks the desk");
    let address = first.address;
    let (answered, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(reset(address, Some(&guest_world())));
    });
    std::thread::sleep(Duration::from_millis(300));
    release.send(()).expect("the desk is released");
    let (status, body) = answer.recv_timeout(ANSWER).expect("the reset answers");
    assert_eq!(status, 200, "{body}");
    assert_eq!(role_ids(&first), ["guest"]);
    assert_eq!(marker(state)["done"], 3, "the world marks every entry done");
    first.stop();

    let mut second = serve(&arguments);
    second.line("already ran");
    assert_eq!(counted(second.address), [0, 0, 0, 0]);
    assert_eq!(role_ids(&second), ["guest"]);
    let stderr = second.stop();
    assert!(
        stderr.iter().all(|line| !line.contains(" ended: ")),
        "{stderr:#?}"
    );
    assert!(asked.try_recv().is_err(), "the desk was asked again");
}

/// A state written before the marker existed, rows and no `seed.json`: the seed is applied as it
/// was then, every entry sent, and `seed.json` then records every entry done.
#[test]
fn a_state_with_rows_and_no_marker_is_seeded_then_marked() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            grant("auditor", "north", "seed-2"),
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
    std::fs::remove_file(Path::new(state).join(MARKER)).expect("the marker is removed");

    let mut second = serve(&arguments);
    let ended = second.ended();
    assert!(
        ended.contains("2 sent, 0 applied, 2 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(role_ids(&second), ["admin", "auditor"]);
    let recorded = marker(state);
    assert_eq!(recorded["done"], 2, "{recorded}");
    assert_eq!(recorded["digest"], json!(digest(&seed)), "{recorded}");
}

/// Another seed on a state whose marker exists but which holds no row: it runs from its first
/// entry, and the marker then records it.
#[test]
fn a_changed_seed_on_a_state_with_no_row_runs_from_its_first_entry() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
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
    assert!(
        ended.contains("1 sent, 0 applied, 0 already there, 1 refused"),
        "{ended}"
    );
    first.stop();
    let recorded = digest(&seed);
    assert_eq!(marker(state)["digest"], json!(recorded));

    write(
        &directory,
        "seed.json",
        &json!([grant("admin", "north", "seed-1")]).to_string(),
    );
    let mut second = serve(&arguments);
    let ended = second.ended();
    assert!(
        ended.contains("1 sent, 1 applied, 0 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(role_ids(&second), ["admin"]);
    let marked = marker(state);
    assert_eq!(marked["digest"], json!(digest(&seed)), "{marked}");
    assert_eq!(marked["done"], 1, "{marked}");
    let stderr = second.stop();
    assert!(
        stderr.iter().any(|line| line.contains(&recorded)),
        "the start names the digest the state recorded: {stderr:#?}"
    );
}

/// `seed.json` is readable and writable by its owner alone.
#[cfg(unix)]
#[test]
fn the_seed_marker_is_private() {
    use std::os::unix::fs::PermissionsExt as _;

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
    let mode = std::fs::metadata(Path::new(state).join(MARKER))
        .expect("the marker exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "{mode:o}");
}

/// A seed of three entries stopped by SIGTERM while its second is held: once that entry is done the
/// run exits 143, and a restart with the same seed sends the third alone.
#[test]
fn a_run_stopped_mid_seed_resumes_after_the_last_entry_done() {
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
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--observe",
    ];
    let mut first = serve(&arguments);
    asked
        .recv_timeout(ANSWER)
        .expect("the second entry asks the desk");
    signal(&first, rustix::process::Signal::TERM);
    std::thread::sleep(Duration::from_millis(200));
    release.send(()).expect("the desk is released");
    assert_eq!(exited(&mut first).code(), Some(143));
    assert_eq!(marker(state)["done"], 2);
    first.stop();

    let mut second = serve(&arguments);
    let ended = second.ended();
    assert!(
        ended.contains("1 sent, 1 applied, 0 already there, 0 refused"),
        "the third entry alone: {ended}"
    );
    assert_eq!(role_ids(&second), ["admin", "auditor"]);
    assert_eq!(of(&second, "vault.access.Vouched"), 1);
    assert_eq!(marker(state)["done"], 3);
    assert!(asked.try_recv().is_err(), "the desk was asked again");
}

/// On a state whose seed ran to its end, a reset whose first seed entry is held is killed: the
/// reset recorded the count 0 before it sent that entry, so a restart with the same seed applies it
/// from its first entry.
#[test]
fn a_kill_during_a_resets_first_entry_restarts_the_seed() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let (desk, asked, release) = holding_desk();
    let seed = write(
        &directory,
        "seed.json",
        &json!([vouch("v-1", "seed-1"), grant("admin", "north", "seed-2")]).to_string(),
    );
    let document = environment(&directory, "sqlite", Some(desk), "");
    let arguments = [
        "--environment",
        &document,
        "--state",
        state,
        "--seed",
        &seed,
        "--observe",
    ];
    let mut first = serve(&arguments);
    asked.recv_timeout(ANSWER).expect("the start's seed asks");
    release.send(()).expect("the desk is released");
    let ended = first.ended();
    assert!(ended.contains("2 sent, 2 applied"), "{ended}");
    assert_eq!(marker(state)["done"], 2);
    let address = first.address;
    std::thread::spawn(move || {
        let _ = reset(address, None);
    });
    asked.recv_timeout(ANSWER).expect("the reset's seed asks");
    assert_eq!(
        marker(state)["done"],
        0,
        "the reset recorded 0 before its first entry"
    );
    first.stop();
    release.send(()).expect("the desk is released");

    let mut second = serve(&arguments);
    asked
        .recv_timeout(ANSWER)
        .expect("the restart's seed asks from its first entry");
    release.send(()).expect("the desk is released");
    let ended = second.ended();
    assert!(
        ended.contains("2 sent, 2 applied, 0 already there, 0 refused"),
        "{ended}"
    );
    assert_eq!(role_ids(&second), ["admin"]);
    assert_eq!(of(&second, "vault.access.Vouched"), 1);
    assert_eq!(marker(state)["done"], 2);
}
