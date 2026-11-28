//! Adversary of wave 32 U4 (`story:seed-under-a-lease`, D3 A): the write of the seed's record
//! lands only while the lease row names the writer's token, the row held locked while it writes.
//! A write of the record that is in flight while another process's take of the lease holds the
//! lease row, and that take then commits a later token, lands nothing.
//!
//! Round 2 (`Lease::move_seed`): a move of the record from one component's lease table to
//! another's lands nothing, in either table, once the lease of the table it moves to was taken,
//! or while a take of the lease of the table it moves from is in flight and then commits.

/// The PostgreSQL lane (`task check:postgres`).
#[cfg(feature = "postgres")]
mod postgres {
    use std::time::{Duration, Instant};

    use serde_json::json;
    use spindle_kernel::{Database, Lease, postgres_prefix};

    fn database() -> Database {
        let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
            .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
        Database::new(url)
    }

    /// `stem` and letters no earlier run drew.
    fn fresh(stem: &str) -> String {
        let mut value = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
            ^ u128::from(std::process::id()) << 64;
        let mut letters = String::new();
        for _ in 0..12 {
            letters.push(char::from(
                b'a' + u8::try_from(value % 26).expect("below 26"),
            ));
            value /= 26;
        }
        format!("{stem}-{letters}")
    }

    /// Another session moves the token of the lease row of `table` on, as a take does, and holds
    /// the row until `held` has passed before it commits.
    fn taking(url: String, table: String, held: Duration) -> std::thread::JoinHandle<()> {
        let (locked, row) = std::sync::mpsc::channel();
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
                .block_on(transaction.simple_query(&format!(
                    "UPDATE {table} SET body = jsonb_set(body, '{{token}}', \
                     to_jsonb((body->>'token')::bigint + 1)) WHERE row_key = 'lease'"
                )))
                .expect("the token moves on");
            locked.send(()).expect("the test waits");
            std::thread::sleep(held);
            runtime
                .block_on(transaction.commit())
                .expect("the take commits");
            drop(client);
            let _ = runtime.block_on(driver);
        });
        row.recv().expect("the lease row is held");
        holder
    }

    /// The record's write starts while the take holds the lease row, waits for it, and is refused
    /// once the take commits token 2; the row stays as the holder of token 1 last wrote it.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_record_write_in_flight_during_a_takeover_lands_nothing() {
        let database = database();
        let component = fresh("inflight");
        // A statement on the lease connection waits a third of the term for a lock: 1 s.
        let mut lease =
            Lease::connect(&database, Duration::from_secs(3)).expect("A connects for the lease");
        let token = lease
            .take(&component)
            .expect("the lease is read")
            .expect("no process holds the lease: A takes it");
        let first = json!({"digest": "sha256:aa", "done": 1, "rejected": []});
        lease
            .record_seed(&component, token, &first)
            .expect("A writes the record under its lease");
        let table = format!(
            "{}.{}_p_lease",
            database.schema,
            postgres_prefix(&component)
        );
        let held = Duration::from_millis(300);
        let take = taking(database.url.clone(), table, held);
        let started = Instant::now();
        let written = lease.record_seed(
            &component,
            token,
            &json!({"digest": "sha256:aa", "done": 2, "rejected": []}),
        );
        let waited = started.elapsed();
        take.join().expect("the take ends");
        assert!(
            waited >= held / 2,
            "the write was in flight, waiting for the take's lock: {waited:?}"
        );
        let refused = written
            .expect_err("the write is refused once the take committed token 2")
            .to_string();
        assert!(
            refused.contains("no longer names token 1"),
            "the refusal names the lease: {refused}"
        );
        assert_eq!(
            lease.seed_record(&component).expect("the row is read"),
            Some(first),
            "the record is as token 1 last wrote it"
        );
    }

    /// A holds the leases of `old` and `new` and the record is in `old`'s table; B takes the lease
    /// of `new`, the table the record moves to. A's move is refused, and neither row changes.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_move_is_refused_once_the_lease_it_moves_to_was_taken() {
        let database = database();
        let old = fresh("moving");
        let new = fresh("moving");
        let term = Duration::from_secs(1);
        let mut first = Lease::connect(&database, term).expect("A connects for the leases");
        let old_token = first
            .take(&old)
            .expect("the lease is read")
            .expect("A takes the old lease");
        let new_token = first
            .take(&new)
            .expect("the lease is read")
            .expect("A takes the new lease");
        let recorded = json!({"digest": "sha256:cc", "done": 1, "rejected": []});
        first
            .record_seed(&old, old_token, &recorded)
            .expect("A records the seed in the old table");
        let mut second = Lease::connect(&database, term).expect("B connects for the lease");
        let deadline = Instant::now() + Duration::from_secs(10);
        while second.take(&new).expect("the lease is read").is_none() {
            assert!(Instant::now() < deadline, "B never took the new lease");
            std::thread::sleep(Duration::from_millis(100));
        }
        let moved = first.move_seed(
            (&old, old_token),
            (&new, new_token),
            &json!({"digest": "sha256:cc", "done": 2, "rejected": []}),
        );
        assert!(
            moved.is_err(),
            "A's move is refused once B holds the lease of the table it moves to"
        );
        assert_eq!(
            second.seed_record(&old).expect("read"),
            Some(recorded),
            "the old row is as it was"
        );
        assert_eq!(
            second.seed_record(&new).expect("read"),
            None,
            "nothing was moved in"
        );
    }

    /// The move starts while a take of the lease of `old`, the table it moves from, holds that
    /// lease row, waits for it, and lands nothing in either table once the take commits a later
    /// token: one statement, so no row is lost and none is doubled.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_move_in_flight_during_a_takeover_of_its_source_lands_nothing() {
        let database = database();
        let old = fresh("inflight");
        let new = fresh("inflight");
        let mut lease =
            Lease::connect(&database, Duration::from_secs(3)).expect("A connects for the leases");
        let old_token = lease
            .take(&old)
            .expect("the lease is read")
            .expect("A takes the old lease");
        let new_token = lease
            .take(&new)
            .expect("the lease is read")
            .expect("A takes the new lease");
        let recorded = json!({"digest": "sha256:dd", "done": 1, "rejected": []});
        lease
            .record_seed(&old, old_token, &recorded)
            .expect("A records the seed in the old table");
        let table = format!("{}.{}_p_lease", database.schema, postgres_prefix(&old));
        let held = Duration::from_millis(300);
        let take = taking(database.url.clone(), table, held);
        let started = Instant::now();
        let moved = lease.move_seed(
            (&old, old_token),
            (&new, new_token),
            &json!({"digest": "sha256:dd", "done": 2, "rejected": []}),
        );
        let waited = started.elapsed();
        take.join().expect("the take ends");
        assert!(
            waited >= held / 2,
            "the move was in flight, waiting for the take's lock: {waited:?}"
        );
        assert!(
            moved.is_err(),
            "the move is refused once the take committed"
        );
        assert_eq!(
            lease.seed_record(&old).expect("read"),
            Some(recorded),
            "the old row stays"
        );
        assert_eq!(
            lease.seed_record(&new).expect("read"),
            None,
            "nothing was moved in"
        );
    }

    /// Round 3 (`Lease::gather_seed`): three tables, the record alike in each; B takes the lease
    /// of the middle one, neither the table gathered into nor the last. The gathering is refused
    /// and every row stays as it was.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_gathering_is_refused_once_the_middle_lease_was_taken() {
        let database = database();
        let [into, middle, last] = [fresh("gather"), fresh("gather"), fresh("gather")];
        let term = Duration::from_secs(1);
        let mut first = Lease::connect(&database, term).expect("A connects for the leases");
        let mut tokens = Vec::new();
        for component in [&into, &middle, &last] {
            tokens.push(
                first
                    .take(component)
                    .expect("the lease is read")
                    .expect("A takes the lease"),
            );
        }
        let recorded = json!({"digest": "sha256:ee", "done": 1, "rejected": []});
        for (component, token) in [&into, &middle, &last].into_iter().zip(&tokens) {
            first
                .record_seed(component, *token, &recorded)
                .expect("A records the seed");
        }
        let mut second = Lease::connect(&database, term).expect("B connects for the lease");
        let deadline = Instant::now() + Duration::from_secs(10);
        while second.take(&middle).expect("the lease is read").is_none() {
            assert!(Instant::now() < deadline, "B never took the middle lease");
            std::thread::sleep(Duration::from_millis(100));
        }
        let gathered = first.gather_seed(
            &[(&middle, tokens[1]), (&last, tokens[2])],
            (&into, tokens[0]),
            &json!({"digest": "sha256:ee", "done": 2, "rejected": []}),
        );
        assert!(
            gathered.is_err(),
            "the gathering is refused once B holds the middle lease"
        );
        for component in [&into, &middle, &last] {
            assert_eq!(
                second.seed_record(component).expect("read"),
                Some(recorded.clone()),
                "the row of {component} is as it was"
            );
        }
    }
}
