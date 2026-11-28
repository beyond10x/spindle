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

/// Whether `audit`'s lease is free now: a process of release 1 takes it at once, and gives it back.
/// A lease a reset took for the removal after it and kept would read as held.
fn audit_is_free(schema: &str) -> bool {
    let mut database = spindle_kernel::Database::new(super::super::owner_url());
    database.schema = schema.to_owned();
    let mut probe = spindle_kernel::Lease::connect(&database, Duration::from_secs(60))
        .expect("the probe connects for its leases");
    let Some(token) = probe.take("audit").expect("the lease is read") else {
        return false;
    };
    probe
        .release("audit", token)
        .expect("the probe gives the lease back");
    true
}

/// Release 1 seeds `s1` and `s2` and is stopped, its record in `audit`'s lease table. Answers
/// that record.
fn seeded_by_release_one(environment: &Path, schema: &str, seed: &Path) -> Value {
    let mut first = Replica::start(grown_seeded(environment, schema, seed));
    assert!(
        first
            .line_by("ended: 2 sent, 2 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    recorded(schema, "audit").expect("the seed is recorded")
}

/// Round 1's case, on the coordinator's decision: a reset by a leased run of release 2 without a
/// seed, while a standby of release 1 holds `audit`'s lease, is refused `409` naming `audit`'s
/// table, and empties nothing: `board`'s rows and `audit`'s record stay. Once that standby has
/// ended, a reset that fails on its body and then a plain one each give `audit`'s lease back
/// before they answer; the plain one removes the record, and the next start of release 2 with the
/// same seed sends the whole seed to the emptied store (`story:seed-under-a-lease`, D6 A).
/// About 6 s on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_while_an_old_release_holds_the_removed_components_lease_is_refused_until_it_ends_then_the_seed_is_sent_again()
 {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let record = seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");

    let (mut old, token) = old_release_holding_audit(&schema);
    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = super::super::call(unseeded.address, "POST", "/api/reset", None);
    assert_eq!(status, 409, "{body}");
    let refused = body["refused"].as_str().unwrap_or_default();
    assert!(
        refused.contains(&audit) && refused.contains("another process holds"),
        "the refusal names audit's table and its holder: {body}"
    );
    assert_eq!(task_ids(&unseeded), ["s1", "s2"], "no store was emptied");
    assert_eq!(recorded(&schema, "audit"), Some(record.clone()));
    assert_eq!(recorded(&schema, "board"), None);
    old.release("audit", token)
        .expect("the old release's standby ends");
    drop(old);

    let (status, body) = super::super::call(
        unseeded.address,
        "POST",
        "/api/reset",
        Some(&json!({"rows": "not a list of rows"})),
    );
    assert!(
        status != 200 && status != 409,
        "a world that is no world is refused for itself: {status} {body}"
    );
    assert!(
        audit_is_free(&schema),
        "a reset that failed gave audit's lease back before it answered"
    );
    assert_eq!(task_ids(&unseeded), ["s1", "s2"]);
    assert_eq!(recorded(&schema, "audit"), Some(record));

    let (status, body) = super::super::call(unseeded.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{body}");
    assert!(
        audit_is_free(&schema),
        "the reset gave audit's lease back once the record was removed, while the run runs on"
    );
    assert_eq!(task_ids(&unseeded), Vec::<String>::new());
    unseeded.stop();
    assert_eq!(recorded(&schema, "audit"), None);
    assert_eq!(recorded(&schema, "board"), None);

    let mut next = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        next.line_by("ended: 2 sent, 2 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "the whole seed is sent to the emptied store: {}",
        next.printed()
    );
    next.ready_by(Instant::now() + super::ANSWER);
    assert_eq!(task_ids(&next), ["s1", "s2"], "{}", next.printed());
    next.stop();
}

/// The removal of `audit`'s record after a reset that emptied the stores fails, as it does where
/// the lease the reset took for it was lost meanwhile (the unit's `forget_claimed`, a deposed
/// removal) or its statement failed: here a trigger the owner puts on `audit`'s lease table
/// refuses that one delete, and is dropped after. The reset answers and the stores are empty,
/// with `audit`'s record left, so the next start of release 2 with the same seed reads that
/// record, takes the seed as already run and sends nothing, against D6 A of
/// `story:seed-under-a-lease` ("a later start with a seed treats the store as one with no
/// record"). About 5 s on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_removal_of_the_old_record_that_fails_after_the_reset_leaves_the_seed_to_be_sent_again() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");
    super::super::sql(&[
        format!(
            "CREATE FUNCTION {schema}.refuse_seed_removal() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'the removal of the seed record failed'; END $$"
        ),
        format!(
            "CREATE TRIGGER refuse_seed_removal BEFORE DELETE ON {audit} FOR EACH ROW WHEN \
             (OLD.row_key = 'seed') EXECUTE FUNCTION {schema}.refuse_seed_removal()"
        ),
    ]);

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = super::super::call(unseeded.address, "POST", "/api/reset", None);
    let tasks = task_ids(&unseeded);
    unseeded.stop();
    let reset_printed = unseeded.printed();
    super::super::sql(&[format!("DROP TRIGGER refuse_seed_removal ON {audit}")]);

    let mut next = Replica::start(seeded(&environment, &schema, &seed));
    next.ready_by(Instant::now() + super::ANSWER);
    let next_tasks = task_ids(&next);
    let printed = next.printed();
    next.stop();
    assert!(
        tasks == ["s1", "s2"] || next_tasks == ["s1", "s2"],
        "the reset answered {status} {body} and left {tasks:?}; audit's table holds {:?}; the \
         next seeded start left {next_tasks:?}.\nreset run printed:\n{reset_printed}\nnext start \
         printed:\n{printed}",
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

/// `run` of release 2 without a seed, its state in `schema`, with a lease of 1 s: a reset that
/// waits 1.4 s outlasts the lease it takes for `audit`'s table unless that lease is renewed.
fn short_leased(environment: &Path, schema: &str) -> Command {
    let url = super::super::owner_url();
    let environment = environment.to_str().expect("a UTF-8 path");
    super::super::spindle(
        &[
            "run",
            super::SPEC,
            "--http",
            super::BINDING,
            "--environment",
            environment,
        ],
        &[
            ("database_url", url.as_str()),
            ("database_schema", schema),
            ("database_lease", "1"),
        ],
    )
}

/// How many tasks the world of [`a_reset_that_outlasts_the_lease_keeps_the_removed_components_lease_and_removes_its_record`]
/// loads: enough that writing them, one transaction each, outlasts a lease of 1 s.
const WORLD_TASKS: usize = 1_500;

/// The lease of `audit`'s table a reset takes before it empties anything is renewed while the
/// reset runs (the unit's `Lease::renew_orphan` in the lease thread): a reset that outlasts the
/// lease, here one loading a world of [`WORLD_TASKS`] tasks under a lease of 1 s, keeps it from a
/// standby of release 1 asking for it all along, and removes the record. Without the renewal the
/// standby takes the lease once its term ran out, the removal is deposed, and the record stays
/// beside the emptied store. Green on the unit's head: it pins a guard no other case reaches.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_that_outlasts_the_lease_keeps_the_removed_components_lease_and_removes_its_record() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");
    let until = || -> f64 {
        super::super::sql(&[format!(
            "SELECT (body->>'until')::float8 - extract(epoch FROM clock_timestamp())::float8 \
             FROM {audit} WHERE row_key = 'lease'"
        )])
        .first()
        .and_then(|left| left.parse().ok())
        .unwrap_or(f64::MIN)
    };
    assert!(
        until() <= 0.0,
        "release 1 gave audit's lease back as it stopped"
    );

    let mut unseeded = Replica::start(short_leased(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let rows: Vec<Value> = (0..WORLD_TASKS)
        .map(|task| {
            json!({
                "entity": "tasks.board.Task",
                "identity": format!("w{task}"),
                "state": "Open",
                "fields": {"title": format!("world {task}")},
            })
        })
        .collect();
    let world = json!({ "rows": rows });
    let address = unseeded.address;
    let began = Instant::now();
    let reset = std::thread::spawn(move || {
        let answer = super::super::call(address, "POST", "/api/reset", Some(&world));
        (answer, began.elapsed())
    });

    let deadline = Instant::now() + super::ANSWER;
    while until() <= 0.0 && !reset.is_finished() {
        assert!(
            Instant::now() < deadline,
            "the reset never took audit's lease"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut database = spindle_kernel::Database::new(super::super::owner_url());
    database.schema.clone_from(&schema);
    let mut standby = spindle_kernel::Lease::connect(&database, Duration::from_secs(60))
        .expect("the old release's standby connects for its leases");
    let mut taken = None;
    while !reset.is_finished() && taken.is_none() {
        taken = standby.take("audit").expect("the lease is read");
        std::thread::sleep(Duration::from_millis(20));
    }
    let ((status, body), took) = reset.join().expect("the reset answers");
    let printed = unseeded.printed();
    let tasks = task_ids(&unseeded).len();
    unseeded.stop();
    assert_eq!(status, 200, "{body}\n{printed}");
    assert!(tasks > 0, "the world is loaded");
    assert!(
        took > Duration::from_millis(1_500),
        "the reset outlasts the lease of 1 s: it took {took:?}"
    );
    assert_eq!(
        (taken, recorded(&schema, "audit")),
        (None, None),
        "the standby never took audit's lease while the reset ran {took:?}, and the record is \
         gone:\n{printed}"
    );
}
