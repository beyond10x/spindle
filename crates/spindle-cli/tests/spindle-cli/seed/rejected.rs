//! A seed entry that did not run stops the run (`story:run-fails-on-rejected-line`). One rejected
//! when the file is read exits 1 before listening; one admitted then that fails with a host or
//! execute error once submitted is counted `rejected` in the end line, apart from the entries a
//! declared refusing outcome answered (`refused`), and the run exits 1 once the seed ends. An entry
//! a declared refusal answers keeps the run serving.

use std::time::Instant;

use serde_json::json;

use super::{
    ANSWER, GRANT, OPERATOR, RETIRE, SPEC, Served, VISITOR, environment, grant, role_ids, scratch,
    serve, start, write,
};

/// The exit code of `served` once it exits by itself, within [`ANSWER`].
fn exited(served: &mut Served) -> Option<i32> {
    let deadline = Instant::now() + ANSWER;
    loop {
        if let Some(status) = served.child.try_wait().expect("the run is polled") {
            return status.code();
        }
        assert!(
            Instant::now() < deadline,
            "the run did not exit within {ANSWER:?}: {:#?}",
            served.seen
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A seed whose second entry is not admitted (an input field the command does not declare) exits
/// 1 before listening, naming the entry, and serves nothing.
#[test]
fn a_seed_with_a_rejected_entry_stops_the_run() {
    let directory = scratch();
    let document = environment(&directory, "memory", None, "");
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-1"),
            {"command": GRANT, "input": {"role_id": "a", "title": "A", "rank": 1},
             "actor": OPERATOR, "caller": {"tenant": "north"}, "request_id": "seed-2"},
        ])
        .to_string(),
    );
    let stopped = start(
        SPEC,
        &[
            "--environment",
            &document,
            "--seed",
            &seed,
            "--answer-default",
        ],
    );
    let Err((code, stdout, stderr)) = stopped else {
        panic!("the run listened with a seed entry it does not admit");
    };
    assert_eq!(code, Some(1), "{stderr}");
    assert!(!stdout.contains("listening"), "nothing is served: {stdout}");
    assert!(
        stderr.contains("entry 2") && stderr.contains(&seed),
        "the entry is named: {stderr}"
    );
}

/// An entry admitted at load whose execution fails with a host or execute error (here a caller
/// the command does not grant, `not_granted`) is counted `rejected` in the end line, apart from
/// the declared refusal (`refused`); the run names it on stderr and exits 1 once the seed ends.
#[test]
fn a_seed_entry_rejected_at_run_time_exits_1() {
    let directory = scratch();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            grant("admin", "north", "seed-admin"),
            {"command": GRANT, "input": {"role_id": "spy", "title": "Spy"}, "actor": VISITOR,
             "request_id": "seed-visitor-grant"},
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
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
    ]);
    let ended = served.ended();
    assert!(
        ended.contains("3 sent, 1 applied, 0 already there, 1 refused, 1 rejected"),
        "{ended}"
    );
    assert_eq!(exited(&mut served), Some(1), "{:#?}", served.seen);
    let stderr = served.stop();
    let rejected = stderr
        .iter()
        .find(|line| line.contains("seed-visitor-grant"))
        .unwrap_or_else(|| panic!("the rejection is printed: {stderr:#?}"));
    assert!(
        rejected.contains("entry 2")
            && rejected.contains(GRANT)
            && rejected.contains("rejected: not_granted"),
        "{rejected}"
    );
    let refused = stderr
        .iter()
        .find(|line| line.contains("seed-retire-ghost"))
        .unwrap_or_else(|| panic!("the refusal is printed: {stderr:#?}"));
    assert!(
        refused.contains("refused: vault.access.RoleMissing"),
        "{refused}"
    );
}

/// An entry a declared refusing outcome answers is counted `refused`, and the run keeps serving.
#[test]
fn a_seed_entry_answered_by_a_declared_refusal_keeps_serving() {
    let directory = scratch();
    let seed = write(
        &directory,
        "seed.json",
        &json!([
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
            grant("admin", "north", "seed-admin"),
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
    ]);
    let ended = served.ended();
    assert!(
        ended.contains("2 sent, 1 applied, 0 already there, 1 refused, 0 rejected"),
        "{ended}"
    );
    assert_eq!(
        role_ids(&served),
        ["admin"],
        "the run keeps serving after the refusal"
    );
    assert!(
        served
            .child
            .try_wait()
            .expect("the run is polled")
            .is_none(),
        "the run is still running"
    );
}
