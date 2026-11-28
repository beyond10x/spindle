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
/// About 2 s on the lane.
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
/// record"). About 2 s on the lane.
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
/// lasts over 1 s after it took `audit`'s lease outlasts that lease unless it is renewed.
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
/// loads, each written in a transaction of its own that a trigger holds [`WORLD_PAUSE`]: at least
/// 1.6 s of loading after the reset took `audit`'s lease of 1 s, whatever the load on the server.
const WORLD_TASKS: usize = 200;

/// What a trigger on board's tables sleeps in each statement that writes a row, in seconds.
const WORLD_PAUSE: &str = "0.008";

/// `POST /api/reset` with `body` at `address`, waiting up to 40 s for the answer: a case that
/// judges the lease rows while the reset runs needs no bound on the answer below the test's own.
fn reset_within(address: std::net::SocketAddr, body: Option<&Value>) -> (u16, Value) {
    use std::io::{Read, Write};
    let patience = Duration::from_secs(40);
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, patience).expect("the listener accepts");
    stream
        .set_read_timeout(Some(patience))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: \
         application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, text) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(text).unwrap_or_else(|error| panic!("{error}: {text:?}"))
    };
    (status, body)
}

/// One reading of a lease row: its token, its `until`, and the server's clock as it was read, in
/// seconds since the epoch. `until` 0 is a lease released ([`spindle_kernel::Lease::release`]);
/// any other `until` at or before the clock is a lease that ran out.
type Reading = (i64, f64, f64);

/// Reads the lease row of `table` every 10 ms on one connection of the database's owner, which
/// takes no lock, until `stop` is set, and once more after: what another process's take would have
/// found at each moment, without taking anything.
fn watching(
    table: String,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<Vec<Reading>> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let (client, connection) = runtime
            .block_on(tokio_postgres::connect(
                &super::super::owner_url(),
                tokio_postgres::NoTls,
            ))
            .expect("the test database accepts its owner");
        runtime.spawn(connection);
        let query = format!(
            "SELECT (body->>'token')::bigint, (body->>'until')::float8, \
             extract(epoch FROM clock_timestamp())::float8 FROM {table} WHERE row_key = 'lease'"
        );
        let mut readings = Vec::new();
        loop {
            let last = stop.load(std::sync::atomic::Ordering::SeqCst);
            let row = runtime
                .block_on(client.query_one(&query, &[]))
                .expect("the lease row is read");
            readings.push((row.get(0), row.get(1), row.get(2)));
            if last {
                return readings;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })
}

/// The lease of `audit`'s table a reset takes before it empties anything is renewed while the
/// reset runs (the unit's `Lease::renew_orphan` in the lease thread), so no process of release 1
/// could take it meanwhile: read every 10 ms, without taking it, the lease row names the reset's
/// token with an end ahead of the server's clock from the take until the release (`until` 0) the
/// reset makes before it answers, and never one that ran out. The reset loads a world of
/// [`WORLD_TASKS`] tasks, each write held [`WORLD_PAUSE`] by a trigger, under a lease of 1 s, so it
/// outlasts the lease whatever the load; the case asserts it did. It judges only the lease row
/// before the release, not what a standby takes after the release and before the answer reaches
/// the test (round 3: a standby polling with `take` took the lease in that window, which is
/// legitimate). Red with the renewal of claimed leases removed from `Leases::renewing`. About 4 s
/// on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_that_outlasts_the_lease_keeps_the_removed_components_lease_and_removes_its_record() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");

    let mut unseeded = Replica::start(short_leased(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    super::super::sql(&[
        format!(
            "CREATE FUNCTION {schema}.slow_write() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN PERFORM pg_sleep({WORLD_PAUSE}); RETURN NULL; END $$"
        ),
        format!(
            "DO $$ DECLARE name text; BEGIN FOR name IN SELECT tablename FROM pg_tables WHERE \
             schemaname = '{schema}' AND tablename LIKE 'spindle\\_board\\_%' AND tablename <> \
             'spindle_board_p_lease' LOOP EXECUTE format('CREATE TRIGGER slow_write BEFORE \
             INSERT OR UPDATE ON %I.%I FOR EACH STATEMENT EXECUTE FUNCTION %I.slow_write()', \
             '{schema}', name, '{schema}'); END LOOP; END $$"
        ),
    ]);
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

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watch = watching(audit.clone(), stop.clone());
    std::thread::sleep(Duration::from_millis(50));
    let (status, body) = reset_within(unseeded.address, Some(&world));
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let readings = watch.join().expect("the lease row was watched");
    let printed = unseeded.printed();
    let tasks = task_ids(&unseeded).len();
    unseeded.stop();
    assert_eq!(status, 200, "{body}\n{printed}");
    assert!(tasks > 0, "the world is loaded");

    let before = readings.first().expect("a reading before the reset").0;
    let claim = before + 1;
    let mut held = None::<(f64, f64)>;
    let mut released = false;
    for &(token, until, now) in &readings {
        if token == before {
            continue;
        }
        assert_eq!(
            token, claim,
            "audit's lease was taken at token {token} after the reset's {claim}, before the \
             reset released it: {readings:?}\n{printed}"
        );
        if until == 0.0 {
            released = true;
            break;
        }
        let from = held.map_or(now, |(from, _)| from);
        assert!(
            until > now,
            "audit's lease, taken by the reset at token {claim}, ran out {:.3} s after it was \
             first read held, before the reset released it: another process could take it, and \
             write a record beside the emptied store. Reading (token, until, clock) {:?}\n{printed}",
            now - from,
            (token, until, now)
        );
        held = Some((from, now));
    }
    assert!(released, "the reset released audit's lease: {readings:?}");
    let (from, to) = held.expect("the reset took audit's lease while it ran");
    assert!(
        to - from > 1.3,
        "the reset held audit's lease {:.3} s, past its term of 1 s",
        to - from
    );
    assert_eq!(recorded(&schema, "audit"), None, "{printed}");
}

/// A copy of `audit`'s lease table, its lease row and its record, as the lease table of the
/// component `component` that no fixture names. Answers that table.
fn copied_from_audit(schema: &str, component: &str) -> String {
    let audit = lease_table(schema, "audit");
    let copy = lease_table(schema, component);
    super::super::sql(&[
        format!("CREATE TABLE {copy} (LIKE {audit} INCLUDING ALL)"),
        format!(
            "INSERT INTO {copy} (tenant_id, row_key, body) SELECT tenant_id, row_key, body FROM \
             {audit} WHERE row_key IN ('lease', 'seed')"
        ),
    ]);
    copy
}

/// The row `seed` of the lease table `table`, read as the database's owner.
fn record_in(table: &str) -> Option<Value> {
    super::super::sql(&[format!(
        "SELECT body::text FROM {table} WHERE row_key = 'seed'"
    )])
    .first()
    .map(|text| serde_json::from_str(text).expect("the seed row holds JSON"))
}

/// A process's handle on the leases of `schema`, as one of release 1 has.
fn lease_handle(schema: &str) -> spindle_kernel::Lease {
    let mut database = spindle_kernel::Database::new(super::super::owner_url());
    database.schema = schema.to_owned();
    spindle_kernel::Lease::connect(&database, Duration::from_secs(60))
        .expect("the process connects for its leases")
}

/// Whether the lease of the lease table `table` is free now: a process takes it at once by its
/// table name, and gives it back.
fn free(schema: &str, table: &str) -> bool {
    let mut probe = lease_handle(schema);
    let Some(token) = probe.take_orphan(table).expect("the lease is read") else {
        return false;
    };
    probe
        .release_orphan(table, token)
        .expect("the probe gives the lease back");
    true
}

/// Two removed components' tables hold the record, `audit`'s and `retired`'s, and a process of
/// release 1 holds `retired`'s lease. The reset takes `audit`'s lease first, in name order, then
/// finds `retired`'s held: it answers `409` naming `retired`'s table, removes neither record,
/// empties no store, and gives `audit`'s lease back before it answers. Once that process ended, a
/// reset removes both records and holds neither lease after. Green on the unit's head. About 2 s
/// on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_refused_for_the_second_removed_components_lease_gives_the_first_back_and_keeps_both_records()
 {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let record = seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");
    let retired = copied_from_audit(&schema, "retired");
    let mut old = lease_handle(&schema);
    let token = old
        .take_orphan(&retired)
        .expect("the lease is read")
        .expect("release 1's process takes retired's lease");

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = reset_within(unseeded.address, None);
    let audit_free = free(&schema, &audit);
    let tasks = task_ids(&unseeded);
    assert_eq!(status, 409, "{body}\n{}", unseeded.printed());
    assert!(
        body["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains(&retired)),
        "the refusal names retired's table: {body}"
    );
    assert!(audit_free, "the refused reset gave audit's lease back");
    assert_eq!(tasks, ["s1", "s2"], "no store was emptied");
    assert_eq!(record_in(&audit), Some(record.clone()));
    assert_eq!(record_in(&retired), Some(record));

    old.release_orphan(&retired, token)
        .expect("release 1's process ends");
    let (status, body) = reset_within(unseeded.address, None);
    assert_eq!(status, 200, "{body}\n{}", unseeded.printed());
    assert!(free(&schema, &audit), "audit's lease is given back");
    assert!(free(&schema, &retired), "retired's lease is given back");
    unseeded.stop();
    assert_eq!(record_in(&audit), None);
    assert_eq!(record_in(&retired), None);
}

/// Two removed components' tables hold the record, and the removal of the second, `retired`'s,
/// fails (a trigger the owner puts on that table refuses the delete). The reset answers `500`
/// naming `retired`'s table, empties no store, puts `audit`'s removed record back, and holds
/// neither lease after. Green on the unit's head. About 2 s on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_removal_failing_in_the_second_removed_components_table_puts_the_first_back_and_holds_no_lease()
{
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let record = seeded_by_release_one(&environment, &schema, &seed);
    let audit = lease_table(&schema, "audit");
    let retired = copied_from_audit(&schema, "retired");
    super::super::sql(&[
        format!(
            "CREATE FUNCTION {schema}.refuse_seed_removal() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'the removal of the seed record failed'; END $$"
        ),
        format!(
            "CREATE TRIGGER refuse_seed_removal BEFORE DELETE ON {retired} FOR EACH ROW WHEN \
             (OLD.row_key = 'seed') EXECUTE FUNCTION {schema}.refuse_seed_removal()"
        ),
    ]);

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = reset_within(unseeded.address, None);
    let frees = (free(&schema, &audit), free(&schema, &retired));
    let tasks = task_ids(&unseeded);
    unseeded.stop();
    assert_eq!(status, 500, "{body}\n{}", unseeded.printed());
    assert!(
        body["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains(&retired)),
        "the refusal names retired's table: {body}"
    );
    assert_eq!(frees, (true, true), "no lease is held after the answer");
    assert_eq!(tasks, ["s1", "s2"], "no store was emptied");
    assert_eq!(
        record_in(&audit),
        Some(record.clone()),
        "audit's is put back"
    );
    assert_eq!(record_in(&retired), Some(record));
}

/// A reset refused `409` by `console::reset` (a world with a clock, on a run without `--clock`)
/// empties no store, so the unit puts back the record it removed from `audit`'s table before the
/// reset ran. Where that put-back fails (a trigger the owner puts on `audit`'s table refuses the write of
/// the row `seed`, and is dropped after the answer), the part-done record (entry 1 of 3) is gone
/// while the store still holds entry 1: the next start of release 2 with the seed finds a store
/// with rows and no record, takes the seed as run (D5 B) and never sends entries 2 and 3. Before
/// the unit's claim no refused reset touched the record. About 3 s on the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_refused_409_whose_put_back_fails_leaves_the_part_done_seed_to_resume() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let whole = seed_file(directory.path(), "seed.json", &["s1", "s2", "s3"]);
    let schema = super::super::schema();
    let part = left_part_done_in_audit(directory.path(), &environment, &schema, &whole);
    let audit = lease_table(&schema, "audit");
    super::super::sql(&[
        format!(
            "CREATE FUNCTION {schema}.refuse_put_back() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'the write of the seed record failed'; END $$"
        ),
        format!(
            "CREATE TRIGGER refuse_put_back BEFORE INSERT OR UPDATE ON {audit} FOR EACH ROW WHEN \
             (NEW.row_key = 'seed') EXECUTE FUNCTION {schema}.refuse_put_back()"
        ),
    ]);

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = reset_within(
        unseeded.address,
        Some(&json!({"rows": [], "clock": "2030-01-01T00:00:00Z"})),
    );
    let tasks = task_ids(&unseeded);
    unseeded.stop();
    let reset_printed = unseeded.printed();
    super::super::sql(&[format!("DROP TRIGGER refuse_put_back ON {audit}")]);
    assert_eq!(status, 409, "{body}\n{reset_printed}");
    assert_eq!(tasks, ["s1"], "the refused reset emptied no store");

    let mut next = Replica::start(seeded(&environment, &schema, &whole));
    next.ready_by(Instant::now() + super::ANSWER);
    let deadline = Instant::now() + super::ANSWER;
    while !(next.has_line("ended:") || next.has_line("taken as applied"))
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let next_tasks = task_ids(&next);
    let printed = next.printed();
    next.stop();
    assert_eq!(
        next_tasks,
        ["s1", "s2", "s3"],
        "a reset refused 409 changed no store, yet audit's record {part} was not put back \
         (audit's table holds {:?}, board's {:?}), so the next seeded start took the store as \
         seeded and never sent entries 2 and 3.\nreset run printed:\n{reset_printed}\nnext \
         start printed:\n{printed}",
        record_in(&audit),
        recorded(&schema, "board")
    );
}

/// The removed components' tables a claim goes through: `audit`'s and three copies of it.
const CLAIMED: [&str; 4] = ["archive", "audit", "retired", "vintage"];

/// What a trigger sleeps in each take of a lease (a change of its token) and each removal of a
/// record in the tables of [`CLAIMED`], in seconds: under the lease connection's statement timeout
/// (a third of the lease of 2 s), and over the run's wait for the lease thread (that timeout times
/// the two components and 2, about 2.67 s) across the eight statements of the claim.
const CLAIM_PAUSE: &str = "0.4";

/// The run's thread waits for the claim a bounded time (`SeedRow::asked`, the statement timeout
/// times the components and 2); a claim that runs longer, here over four removed components'
/// tables each statement of which a trigger holds [`CLAIM_PAUSE`], is answered `500` "refused
/// before any store is emptied", and `seed::claimed`'s refusal skips `seed::unclaimed`. The lease
/// thread carries on: it removes every record and keeps every lease it took, renewed with the
/// run's own, after the answer that said "no lease is kept". With the store full, the records gone
/// and the leases held by a run of release 2, a start of release 1 cannot lead those components,
/// and a seeded start of release 2 takes the store as seeded. Red on the unit's head. About 8 s on
/// the lane.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_claim_that_outlasts_the_runs_wait_keeps_no_lease_and_removes_no_record_once_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = super::super::schema();
    let record = seeded_by_release_one(&environment, &schema, &seed);
    let tables: Vec<String> = CLAIMED
        .iter()
        .map(|component| {
            if *component == "audit" {
                lease_table(&schema, component)
            } else {
                copied_from_audit(&schema, component)
            }
        })
        .collect();
    let mut statements = vec![format!(
        "CREATE FUNCTION {schema}.slow_claim() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_sleep({CLAIM_PAUSE}); IF TG_OP = 'DELETE' THEN RETURN OLD; END IF; RETURN \
         NEW; END $$"
    )];
    for table in &tables {
        statements.push(format!(
            "CREATE TRIGGER slow_take BEFORE UPDATE ON {table} FOR EACH ROW WHEN (OLD.row_key = \
             'lease' AND OLD.body->'token' IS DISTINCT FROM NEW.body->'token') EXECUTE FUNCTION \
             {schema}.slow_claim()"
        ));
        statements.push(format!(
            "CREATE TRIGGER slow_removal BEFORE DELETE ON {table} FOR EACH ROW WHEN (OLD.row_key \
             = 'seed') EXECUTE FUNCTION {schema}.slow_claim()"
        ));
    }
    super::super::sql(&statements);

    let mut unseeded = Replica::start(super::run(&environment, &schema));
    unseeded.ready_by(Instant::now() + super::ANSWER);
    let (status, body) = reset_within(unseeded.address, None);
    // The lease thread's claim, eight statements of 0.4 s, ends within about 1 s of the answer.
    std::thread::sleep(Duration::from_millis(1_500));
    let kept: Vec<String> = tables
        .iter()
        .filter(|table| !free(&schema, table))
        .cloned()
        .collect();
    let records: Vec<Option<Value>> = tables.iter().map(|table| record_in(table)).collect();
    let tasks = task_ids(&unseeded);
    let printed = unseeded.printed();
    unseeded.stop();
    if status == 200 {
        assert_eq!(tasks, Vec::<String>::new(), "{body}");
        assert_eq!(records, vec![None; CLAIMED.len()], "{body}");
    } else {
        assert_eq!(
            tasks,
            ["s1", "s2"],
            "a refused reset empties no store: {body}"
        );
        assert_eq!(
            (kept, records),
            (Vec::new(), vec![Some(record); CLAIMED.len()]),
            "the reset answered {status} {body}, refused before any store was emptied, yet the \
             run holds the leases listed after it and the records are as listed:\n{printed}"
        );
    }
}
