//! Adversary of wave 32 U4 (`story:seed-under-a-lease`, D3 A): the write of the seed's record
//! lands only while the lease row names the writer's token, the row held locked while it writes.
//! A write of the record that is in flight while another process's take of the lease holds the
//! lease row, and that take then commits a later token, lands nothing.

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
}
