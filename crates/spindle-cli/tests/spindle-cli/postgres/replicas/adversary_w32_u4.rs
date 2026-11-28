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
