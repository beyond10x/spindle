//! Adversary of wave 32 U4 (`story:seed-under-a-lease`): the seed's record of a run with a lease
//! is the row `seed` of the lease table of the first component in name order (D4 A). A later
//! release of the specification that adds a component whose name sorts before the old first one
//! moves that row to a table where nothing was recorded, while the store keeps its rows: the run
//! then reads the store as one seeded before the record existed (D5 B) and skips the seed, where
//! the record it wrote says otherwise.
//!
//! `tests/fixtures/adversary-w32-u4-grown` is `tests/fixtures/ingress` with one component added,
//! `audit`, which sorts before `board`; `board` and `ledger` are unchanged. Every case reaches a
//! database, so each is ignored without `SPINDLE_TEST_DATABASE_URL`; `task check:postgres` runs
//! them.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::json;

use super::{LEASE, Replica, environment, recorded, seed_file, seeded};

/// The ingress fixture with `audit` added before `board` in name order.
const GROWN: &str = "crates/spindle-cli/tests/fixtures/adversary-w32-u4-grown";

/// `run` on [`GROWN`] with `environment` and `--seed seed`, its state in `schema`, a lease of
/// [`LEASE`].
fn grown_seeded(environment: &Path, schema: &str, seed: &Path) -> Command {
    let url = super::owner_url();
    let environment = environment.to_str().expect("a UTF-8 path");
    let seed = seed.to_str().expect("a UTF-8 path");
    super::spindle(
        &["run", GROWN, "--environment", environment, "--seed", seed],
        &[
            ("database_url", url.as_str()),
            ("database_schema", schema),
            ("database_lease", LEASE),
        ],
    )
}

/// The first release seeds `first.json` whole; the next release adds `audit` and is started with
/// another seed file. The store holds rows and a complete record of another file, so the story's
/// D7 A, the concepts page ("With another seed file, the process exits 1 before it serves a
/// request, naming both digests, unless the store holds no row") and the `--seed` help ask for
/// exit 1 naming both digests. The record is looked up in `audit`'s lease table instead, where
/// there is none, and the new file is taken as applied.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn another_seed_file_after_a_component_sorting_first_was_added_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let first_file = seed_file(directory.path(), "first.json", &["s1"]);
    let other_file = seed_file(directory.path(), "other.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let mut first = Replica::start(seeded(&environment, &schema, &first_file));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let record = recorded(&schema, "board").expect("the seed is recorded");
    let digest = record["digest"].as_str().expect("a digest").to_owned();

    match super::super::started(grown_seeded(&environment, &schema, &other_file)) {
        Ok(served) => panic!(
            "the run with another seed file on a store holding rows and a record of {digest} \
             listened on {} instead of exiting 1 before serving (D7 A); audit's lease table now \
             holds {:?}",
            served.address,
            recorded(&schema, "audit")
        ),
        Err(refused) => {
            assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
            assert!(
                refused.stderr.contains(&digest) && refused.stderr.matches("sha256:").count() >= 2,
                "the refusal names both digests: {}",
                refused.stderr
            );
        }
    }
}

/// A leader of the first release was killed after entry 1 of 3 of `seed.json` (the record reads
/// `done` 1, as `a_killed_leader_resumes_the_seed_after_its_count` leaves it); the next process
/// to lead runs the next release, which adds `audit`, with the same file. The Outcome asks: "with
/// a record of the same file part done it resumes after the count". It skips instead, and the two
/// entries left are never sent.
///
/// The killed leader's state is built: the first release seeds entry 1 alone from a file of one
/// entry, and the record is then set, as the owner, to the three-entry file's digest at `done` 1.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_seed_left_part_done_resumes_after_a_component_sorting_first_was_added() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let entry_one = seed_file(directory.path(), "one.json", &["s1"]);
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    let mut first = Replica::start(seeded(&environment, &schema, &entry_one));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let digest =
        ess_conformance::known_failures::sha256(&std::fs::read(&whole).expect("the seed is read"));
    let part = json!({"digest": digest, "done": 1, "rejected": []});
    super::super::sql(&[format!(
        "UPDATE {schema}.{}_p_lease SET body = '{part}'::jsonb WHERE row_key = 'seed'",
        spindle_kernel::postgres_prefix("board")
    )]);
    assert_eq!(recorded(&schema, "board"), Some(part));

    let next = Replica::start(grown_seeded(&environment, &schema, &whole));
    // `GET /__spindle/ready` answers `200` only once the seed has ended, so the end line of a
    // seed that runs is printed by then.
    next.ready_by(Instant::now() + Duration::from_secs(10));
    assert!(
        next.has_line("ended: 2 sent, 2 applied"),
        "entries 2 and 3 are sent after the count the record holds: {}",
        next.printed()
    );
}

/// The control of the case above: the same record, built the same way, with the specification
/// unchanged. The next process resumes after the count, so the built record is one the run
/// honours, and only the added component makes the case above skip.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_seed_left_part_done_resumes_on_the_same_components() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let entry_one = seed_file(directory.path(), "one.json", &["s1"]);
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    let mut first = Replica::start(seeded(&environment, &schema, &entry_one));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let digest =
        ess_conformance::known_failures::sha256(&std::fs::read(&whole).expect("the seed is read"));
    let part = json!({"digest": digest, "done": 1, "rejected": []});
    super::super::sql(&[format!(
        "UPDATE {schema}.{}_p_lease SET body = '{part}'::jsonb WHERE row_key = 'seed'",
        spindle_kernel::postgres_prefix("board")
    )]);

    let next = Replica::start(seeded(&environment, &schema, &whole));
    next.ready_by(Instant::now() + Duration::from_secs(10));
    assert!(
        next.has_line("ended: 2 sent, 2 applied"),
        "entries 2 and 3 are sent after the count the record holds: {}",
        next.printed()
    );
    let record = recorded(&schema, "board").expect("the seed is recorded");
    assert_eq!(record["done"], 3, "{record}");
}

/// Round 2: a release rolled back and then forward again, every start with the same seed file
/// that has run whole. The release that adds `audit` moves the record into `audit`'s lease table;
/// the release rolled back to does not read that table, takes the store as seeded (D5 B, the case
/// the coordinator parked) and writes a complete record of the same file into `board`'s. Rolled
/// forward again, the run finds two records, equal, of the file it was given: the Outcome asks
/// that "once one process has completed it, it skips", and the acceptance's restart prints the
/// "already ran" line and reads ready. It exits 1 instead, and every restart meets the same exit
/// until a person removes a row.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_release_rolled_back_and_forward_with_the_same_seed_skips_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = super::super::schema();
    let mut first = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();

    let mut forward = Replica::start(grown_seeded(&environment, &schema, &seed));
    forward.ready_by(Instant::now() + super::ANSWER);
    forward.stop();
    let record = recorded(&schema, "audit").expect("the record moved to audit's table");
    assert_eq!(recorded(&schema, "board"), None, "{}", forward.printed());

    let mut back = Replica::start(seeded(&environment, &schema, &seed));
    back.ready_by(Instant::now() + super::ANSWER);
    back.stop();
    assert_eq!(
        recorded(&schema, "board"),
        Some(record.clone()),
        "the release rolled back to records the same file complete: {}",
        back.printed()
    );

    match super::super::started(grown_seeded(&environment, &schema, &seed)) {
        Ok(served) => {
            let (status, body) = super::call(served.address, "GET", "/__spindle/ready", None);
            assert_eq!(status, 200, "{body}");
        }
        Err(refused) => panic!(
            "rolled forward with the same file, both records {record}, the run exited {} \
             instead of skipping the seed: {}",
            refused.status, refused.stderr
        ),
    }
}

/// Round 3: three lease tables hold a record, two alike and one that differs in `done` alone.
/// The run exits 1 before serving, naming the tables, sends no entry, and every row stays as it
/// was: records alike in two tables do not outvote the third.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn records_in_three_tables_two_alike_and_one_different_are_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = super::super::schema();
    let mut first = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let mut forward = Replica::start(grown_seeded(&environment, &schema, &seed));
    forward.ready_by(Instant::now() + super::ANSWER);
    forward.stop();
    let record = recorded(&schema, "audit").expect("the record is in audit's table");
    let mut other = record.clone();
    other["done"] = json!(0);
    let table = |component: &str| {
        format!(
            "{schema}.{}_p_lease",
            spindle_kernel::postgres_prefix(component)
        )
    };
    super::super::sql(&[
        format!(
            "INSERT INTO {} (tenant_id, row_key, body) SELECT tenant_id, row_key, body FROM {} \
             WHERE row_key = 'seed'",
            table("board"),
            table("audit")
        ),
        format!(
            "INSERT INTO {} (tenant_id, row_key, body) SELECT tenant_id, row_key, '{other}'::jsonb \
             FROM {} WHERE row_key = 'seed'",
            table("ledger"),
            table("audit")
        ),
    ]);
    assert_eq!(recorded(&schema, "board"), Some(record.clone()));
    assert_eq!(recorded(&schema, "ledger"), Some(other.clone()));

    let refused = super::super::refused(grown_seeded(&environment, &schema, &seed));
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    for component in ["audit", "board", "ledger"] {
        assert!(
            refused.stderr.contains(&table(component)),
            "the refusal names {}: {}",
            table(component),
            refused.stderr
        );
    }
    assert!(!refused.stderr.contains("ended:"), "{}", refused.stderr);
    assert_eq!(recorded(&schema, "audit"), Some(record.clone()));
    assert_eq!(recorded(&schema, "board"), Some(record));
    assert_eq!(recorded(&schema, "ledger"), Some(other));
}

// `story:seed-record-after-a-component-is-removed`: release 1 is [`GROWN`] (`audit`, `board`,
// `ledger`) and keeps the record in `audit`'s lease table; release 2 is `tests/fixtures/ingress`
// (`board`, `ledger`), which no longer has `audit`. A start that finds no record in a current
// component's table looks for lease tables of no current component by their table name (D2 A).

/// The lease table of `component` in `schema`.
fn lease_table(schema: &str, component: &str) -> String {
    format!(
        "{schema}.{}_p_lease",
        spindle_kernel::postgres_prefix(component)
    )
}

/// Release 1 seeds entry 1 of `whole` alone, and the record in `audit`'s lease table is then set,
/// as the owner, to `whole`'s digest at `done` 1: a leader stopped after entry 1 of 3. Answers
/// that record.
fn left_part_done_in_audit(
    directory: &Path,
    environment: &Path,
    schema: &str,
    whole: &Path,
) -> serde_json::Value {
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
    assert_eq!(recorded(schema, "board"), None);
    part
}

/// The record release 1 left part done in `audit`'s lease table is found by release 2, which no
/// longer has `audit`: it sends entries 2 and 3 alone, and the record ends in `board`'s table at
/// `done` 3, gone from `audit`'s.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_seed_left_part_done_resumes_after_the_component_holding_its_record_was_removed() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    left_part_done_in_audit(directory.path(), &environment, &schema, &whole);

    let mut next = Replica::start(seeded(&environment, &schema, &whole));
    let ended = next
        .line("ended:", Instant::now() + super::ANSWER)
        .unwrap_or_else(|| panic!("the seed never ended: {}", next.printed()));
    assert!(
        ended.contains("ended: 2 sent, 2 applied"),
        "entries 2 and 3 alone are sent: {}",
        next.printed()
    );
    next.stop();
    let record = recorded(&schema, "board").expect("the record is in board's table");
    assert_eq!(record["done"], 3, "{record}");
    assert_eq!(recorded(&schema, "audit"), None, "{}", next.printed());
}

/// Release 2 started with another seed file than the record release 1 left in `audit`'s lease
/// table exits 1 before listening, naming both digests, and the record stays as it was.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn another_seed_file_after_the_component_holding_the_record_was_removed_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let other = seed_file(directory.path(), "other.json", &["s1", "s4"]);
    let schema = super::super::schema();
    let part = left_part_done_in_audit(directory.path(), &environment, &schema, &whole);
    let digest = part["digest"].as_str().expect("a digest").to_owned();
    let other_digest =
        ess_conformance::known_failures::sha256(&std::fs::read(&other).expect("the seed is read"));

    let refused = super::super::refused(seeded(&environment, &schema, &other));
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    assert!(
        !refused.stdout.contains("listening"),
        "no listening line: {}",
        refused.stdout
    );
    assert!(
        refused.stderr.contains(&digest) && refused.stderr.contains(&other_digest),
        "the refusal names both digests: {}",
        refused.stderr
    );
    assert_eq!(recorded(&schema, "audit"), Some(part));
    assert_eq!(recorded(&schema, "board"), None);
}

/// D2 A: a record in a current component's table wins over one in a table of no current
/// component. Release 1 leaves the record part done in `audit`'s table at `done` 1; the test
/// writes a record of the same file at `done` 2 into `board`'s. Release 2 starts from `board`'s:
/// it sends entry 3 alone, which no release stored before, and `audit`'s record stays as it was.
/// A scan on every read (D2 B) would find two records that differ and exit 1.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_record_in_a_current_table_wins_over_an_orphan_record() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    let part = left_part_done_in_audit(directory.path(), &environment, &schema, &whole);
    super::super::sql(&[format!(
        "INSERT INTO {} (tenant_id, row_key, body) SELECT tenant_id, row_key, \
         jsonb_set(body, '{{done}}', '2') FROM {} WHERE row_key = 'seed'",
        lease_table(&schema, "board"),
        lease_table(&schema, "audit")
    )]);
    assert_eq!(
        recorded(&schema, "board").expect("the record is in board's table")["done"],
        2
    );

    let mut next = Replica::start(seeded(&environment, &schema, &whole));
    let ended = next
        .line("ended:", Instant::now() + super::ANSWER)
        .unwrap_or_else(|| panic!("the seed never ended: {}", next.printed()));
    assert!(
        ended.contains("ended: 1 sent, 1 applied"),
        "entry 3 alone is sent: {}",
        next.printed()
    );
    assert_eq!(super::task_ids(&next), ["s1", "s3"], "{}", next.printed());
    next.stop();
    let record = recorded(&schema, "board").expect("the record is in board's table");
    assert_eq!(record["done"], 3, "{record}");
    assert_eq!(recorded(&schema, "audit"), Some(part));
}

/// D3 A: two lease tables of no current component, `audit`'s and `retired`'s, hold records that
/// differ in `done`, and no current table holds one. Release 2 exits 1 before listening, naming
/// both tables, sends no entry, and both records stay as they were.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn differing_records_in_orphan_tables_exit_1_naming_the_tables() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = super::super::schema();
    let mut first = Replica::start(grown_seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let record = recorded(&schema, "audit").expect("the record is in audit's table");
    let audit = lease_table(&schema, "audit");
    let retired = lease_table(&schema, "retired");
    super::super::sql(&[
        format!("CREATE TABLE {retired} (LIKE {audit} INCLUDING ALL)"),
        format!(
            "INSERT INTO {retired} (tenant_id, row_key, body) SELECT tenant_id, row_key, CASE \
             WHEN row_key = 'seed' THEN jsonb_set(body, '{{done}}', '0') ELSE body END FROM \
             {audit} WHERE row_key IN ('lease', 'seed')"
        ),
    ]);
    let other = recorded(&schema, "retired").expect("the copy is in retired's table");
    assert_eq!(other["done"], 0, "{other}");

    let refused = super::super::refused(seeded(&environment, &schema, &seed));
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    for table in [&audit, &retired] {
        assert!(
            refused.stderr.contains(table.as_str()),
            "the refusal names {table}: {}",
            refused.stderr
        );
    }
    assert!(!refused.stderr.contains("ended:"), "{}", refused.stderr);
    assert_eq!(recorded(&schema, "audit"), Some(record));
    assert_eq!(recorded(&schema, "retired"), Some(other));
}

/// A reset by a leased run without a seed removes the record (`story:seed-under-a-lease`, D6 A),
/// also the one release 1 kept in `audit`'s lease table: otherwise the next start of release 2
/// with the seed would find it there and skip the seed on the emptied store.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_without_a_seed_removes_the_record_of_a_removed_component() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = super::super::schema();
    let mut first = Replica::start(grown_seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + super::ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    assert!(recorded(&schema, "audit").is_some(), "the seed is recorded");

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = super::call(unseeded.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{body}");
    unseeded.stop();
    assert_eq!(
        recorded(&schema, "audit"),
        None,
        "the reset removed the record of the removed component: {}",
        unseeded.printed()
    );
}

/// Adversary round 1 of wave 33 U3: while a process of release 1 holds `audit`'s lease, as its
/// standby does during a rollout, a reset by a leased run of release 2 without a seed cannot
/// remove the record in `audit`'s lease table. It is refused `409` before any store is emptied,
/// naming that table and its holder, and the store's rows and every record stay as they were:
/// a reset that emptied the store and left the record would make the next seeded start send
/// nothing (`story:seed-under-a-lease`, D6 A).
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_while_a_removed_components_lease_is_held_is_refused_and_changes_nothing() {
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
    let record = recorded(&schema, "audit").expect("the seed is recorded");

    let mut database = spindle_kernel::Database::new(super::super::owner_url());
    database.schema.clone_from(&schema);
    let mut old = spindle_kernel::Lease::connect(&database, Duration::from_secs(60))
        .expect("the old release's process connects for its leases");
    let token = old
        .take("audit")
        .expect("the lease is read")
        .expect("the old release's process takes audit's lease");

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = super::call(unseeded.address, "POST", "/api/reset", None);
    let refused = body["refused"].as_str().unwrap_or_default();
    assert_eq!(status, 409, "{body}");
    assert!(
        refused.contains(&lease_table(&schema, "audit"))
            && refused.contains("another process holds"),
        "the refusal names audit's table and its holder: {body}"
    );
    assert_eq!(
        super::task_ids(&unseeded),
        ["s1", "s2"],
        "no store was emptied: {}",
        unseeded.printed()
    );
    unseeded.stop();
    assert_eq!(recorded(&schema, "audit"), Some(record));
    assert_eq!(recorded(&schema, "board"), None);
    old.release("audit", token)
        .expect("the old release's process ends");
}

/// Adversary round 2 of wave 33 U3: where the removal of a record after a reset fails, the line
/// names the table whose row was left, not the first component's. Release 1 is the ingress
/// fixture, its record in `board`'s and `ledger`'s lease tables alike; another session holds
/// `ledger`'s row `seed` locked while the reset runs, so its removal waits out its lock timeout,
/// which `board`'s removal does not meet.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_removal_that_fails_after_the_reset_names_the_table_whose_row_was_left() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = super::super::schema();
    let record = super::recorded_twice(&environment, &schema, &seed, None);
    let board = lease_table(&schema, "board");
    let ledger = lease_table(&schema, "ledger");

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let url = super::super::owner_url();
    let statement = format!("SELECT 1 FROM {ledger} WHERE row_key = 'seed' FOR UPDATE");
    let (locked, row) = std::sync::mpsc::channel();
    let (answered, release) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let (mut client, connection) = runtime
            .block_on(tokio_postgres::connect(&url, tokio_postgres::NoTls))
            .expect("a second connection");
        let driver = runtime.spawn(connection);
        let transaction = runtime
            .block_on(client.transaction())
            .expect("a transaction");
        runtime
            .block_on(transaction.simple_query(&statement))
            .expect("ledger's row `seed` is locked");
        locked.send(()).expect("the test waits");
        let _ = release.recv_timeout(super::ANSWER);
        runtime
            .block_on(transaction.commit())
            .expect("the lock is let go");
        drop(client);
        let _ = runtime.block_on(driver);
    });
    row.recv().expect("ledger's row `seed` is locked");
    let (status, body) = super::call(unseeded.address, "POST", "/api/reset", None);
    let _ = answered.send(());
    holder.join().expect("the other session ends");
    assert_eq!(status, 200, "{body}");
    let line = unseeded
        .line("could not be removed", Instant::now() + super::ANSWER)
        .unwrap_or_else(|| panic!("no line says so: {}", unseeded.printed()));
    unseeded.stop();
    assert!(
        line.contains(&ledger) && !line.contains(&board),
        "the line names {ledger}, whose row was left, and not {board}: {line}"
    );
    assert_eq!(recorded(&schema, "board"), None);
    assert_eq!(recorded(&schema, "ledger"), Some(record));
}
