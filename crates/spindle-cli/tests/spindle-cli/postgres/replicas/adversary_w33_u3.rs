//! Adversary of wave 33 U3 (`story:seed-record-after-a-component-is-removed`): the seed's record
//! kept in the lease table of a component a later release removed, while a process of the old
//! release still holds that table's lease, as its standbys do during a rollout: they take the
//! leases in name order, `audit` first, and stand by on `board`.
//!
//! Release 1 is `tests/fixtures/adversary-w32-u4-grown` (`audit`, `board`, `ledger`); release 2
//! is `tests/fixtures/ingress` (`board`, `ledger`). The old release's process is a
//! [`spindle_kernel::Lease`] the test holds on `audit`, as such a standby holds it. Every case
//! reaches a database, so each is ignored without `SPINDLE_TEST_DATABASE_URL`; `task
//! check:postgres` runs them.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{LEASE, Replica, environment, recorded, seed_file, seeded, task_ids};

/// The ingress fixture with `audit` added before `board` in name order.
const GROWN: &str = "crates/spindle-cli/tests/fixtures/adversary-w32-u4-grown";

/// `run` on [`GROWN`] with `environment` and `--seed seed`, its state in `schema`, a lease of
/// [`LEASE`].
fn grown_seeded(environment: &Path, schema: &str, seed: &Path) -> Command {
    let url = super::super::owner_url();
    let environment = environment.to_str().expect("a UTF-8 path");
    let seed = seed.to_str().expect("a UTF-8 path");
    super::super::spindle(
        &["run", GROWN, "--environment", environment, "--seed", seed],
        &[
            ("database_url", url.as_str()),
            ("database_schema", schema),
            ("database_lease", LEASE),
        ],
    )
}

/// The lease table of `component` in `schema`.
fn lease_table(schema: &str, component: &str) -> String {
    format!(
        "{schema}.{}_p_lease",
        spindle_kernel::postgres_prefix(component)
    )
}

/// A process of release 1 that holds `audit`'s lease and nothing else, as a standby of the old
/// release does while a process of release 2 leads: answers its handle and its token.
fn old_release_holding_audit(schema: &str) -> (spindle_kernel::Lease, u64) {
    let mut database = spindle_kernel::Database::new(super::super::owner_url());
    database.schema = schema.to_owned();
    let mut old = spindle_kernel::Lease::connect(&database, Duration::from_secs(60))
        .expect("the old release's process connects for its leases");
    let deadline = Instant::now() + super::ANSWER;
    loop {
        if let Some(token) = old.take("audit").expect("the lease is read") {
            return (old, token);
        }
        assert!(
            Instant::now() < deadline,
            "the old release's process never took audit's lease"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Release 1 seeds entry 1 of `whole` alone and is stopped, and the record in `audit`'s lease
/// table is then set, as the owner, to `whole`'s digest at `done` 1: a leader stopped after entry
/// 1 of 3. Answers that record.
fn left_part_done_in_audit(
    directory: &Path,
    environment: &Path,
    schema: &str,
    whole: &Path,
) -> Value {
    let entry_one = seed_file(directory, "one.json", &["s1"]);
    let mut first = Replica::start(grown_seeded(environment, schema, &entry_one));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let digest =
        ess_conformance::known_failures::sha256(&std::fs::read(whole).expect("the seed is read"));
    let part = json!({"digest": digest, "done": 1, "rejected": []});
    super::super::sql(&[format!(
        "UPDATE {} SET body = '{part}'::jsonb WHERE row_key = 'seed'",
        lease_table(schema, "audit")
    )]);
    assert_eq!(recorded(schema, "audit"), Some(part.clone()));
    part
}

/// A reset by a leased run of release 2 without a seed leaves no seed's entries done
/// (`story:seed-under-a-lease`, D6 A: "a later start with a seed treats the store as one with no
/// record"). While a standby of release 1 holds `audit`'s lease, the reset removes `board`'s rows
/// and leaves the record in `audit`'s lease table (the unit's `forget_orphans`, a warning). Once
/// that standby is gone, the next start of release 2 with the same seed finds no record in a
/// current table, reads `audit`'s, takes the seed as already run, and sends nothing to the
/// emptied store.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_while_an_old_release_holds_the_removed_components_lease_leaves_the_seed_to_be_sent_again()
 {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let mut first = Replica::start(grown_seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 2 sent, 2 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    assert!(recorded(&schema, "audit").is_some(), "the seed is recorded");

    let (mut old, token) = old_release_holding_audit(&schema);
    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = super::super::call(unseeded.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{body}");
    unseeded.stop();
    let reset_printed = unseeded.printed();
    old.release("audit", token)
        .expect("the old release's standby ends");
    drop(old);

    let mut next = Replica::start(seeded(&environment, &schema, &seed));
    next.ready_by(Instant::now() + super::ANSWER);
    let tasks = task_ids(&next);
    let printed = next.printed();
    next.stop();
    assert_eq!(
        tasks,
        ["s1", "s2"],
        "after the reset the whole seed is sent to the emptied store; audit's lease table still \
         holds {:?}.\nreset run printed:\n{reset_printed}\nnext start printed:\n{printed}",
        recorded(&schema, "audit")
    );
}

/// The write path the unit takes while another process holds the old table's lease: the record is
/// written to `board`'s table alone, a line names `audit`'s table, and `audit`'s record stays.
/// A process of the old release that then writes a record of its own into `audit`'s table, under
/// the lease it holds, does not move the next start of release 2, which reads `board`'s (D2 A).
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_record_left_under_an_old_release_lease_is_written_to_the_first_table_and_wins_after() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    let part = left_part_done_in_audit(directory.path(), &environment, &schema, &whole);
    let audit = lease_table(&schema, "audit");

    let (mut old, token) = old_release_holding_audit(&schema);
    let mut next = Replica::start(seeded(&environment, &schema, &whole));
    let ended = next
        .line("ended:", Instant::now() + super::ANSWER)
        .unwrap_or_else(|| panic!("the seed never ended: {}", next.printed()));
    assert!(
        ended.contains("ended: 2 sent, 2 applied"),
        "entries 2 and 3 alone are sent: {}",
        next.printed()
    );
    assert!(
        next.line_by(&audit, Instant::now() + super::ANSWER)
            .is_some()
            && next.has_line("another process holds"),
        "a line names audit's table and its holder: {}",
        next.printed()
    );
    next.stop();
    let record = recorded(&schema, "board").expect("the record is in board's table");
    assert_eq!(record["done"], 3, "{record}");
    assert_eq!(recorded(&schema, "audit"), Some(part.clone()));

    let differing = json!({"digest": part["digest"], "done": 0, "rejected": []});
    old.record_seed("audit", token, &differing)
        .expect("the old release's process writes under the lease it holds");
    assert_eq!(recorded(&schema, "audit"), Some(differing.clone()));

    let mut again = Replica::start(seeded(&environment, &schema, &whole));
    again.ready_by(Instant::now() + super::ANSWER);
    assert!(
        again
            .line_by("already ran", Instant::now() + super::ANSWER)
            .is_some()
            && !again.has_line("ended:"),
        "board's record wins and no entry is sent: {}",
        again.printed()
    );
    assert_eq!(task_ids(&again), ["s1", "s2", "s3"], "{}", again.printed());
    again.stop();
    assert_eq!(
        recorded(&schema, "board").expect("board's record")["done"],
        3
    );
    assert_eq!(recorded(&schema, "audit"), Some(differing));
    old.release("audit", token)
        .expect("the old release's process ends");
}
