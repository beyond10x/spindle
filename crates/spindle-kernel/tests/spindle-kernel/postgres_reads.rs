//! The rows PostgreSQL read from a component's event table, from its statistics: a count of work
//! that load on the machine does not change, where a time does (`story:postgres-lane-margin`).

use std::time::{Duration, Instant};

/// The first column of the first row `query` answers on `url`.
pub fn sql_value(url: &str, query: &str) -> String {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
            .await
            .expect("the test database accepts its owner");
        let driver = tokio::spawn(connection);
        let value = client
            .simple_query(query)
            .await
            .unwrap_or_else(|error| panic!("{query}: {error}"))
            .into_iter()
            .find_map(|message| match message {
                tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{query}: no row"));
        drop(client);
        let _ = driver.await;
        value
    })
}

/// `url` with the application name `tag`, which every session opened through it carries in
/// `pg_stat_activity`.
pub fn tagged(url: &str, tag: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}application_name={tag}")
}

/// The rows read so far from `component`'s event table in `schema`, by a sequential scan or from
/// its indexes, once every session of [`tagged`] `tag` on `url`'s server has ended. From
/// PostgreSQL 15 a session hands its statistics over before it leaves `pg_stat_activity`; before
/// 15 a collector process takes them up to half a second later, so the wait adds 2.5 s there.
pub fn events_read(url: &str, schema: &str, component: &str, tag: &str) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while sql_value(
        url,
        &format!("SELECT count(*) FROM pg_stat_activity WHERE application_name = '{tag}'"),
    ) != "0"
    {
        assert!(
            Instant::now() < deadline,
            "the sessions of {tag} did not end within 10 s"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let version: u32 = sql_value(url, "SHOW server_version_num")
        .parse()
        .expect("a version number");
    if version < 150_000 {
        std::thread::sleep(Duration::from_millis(2500));
    }
    let table = format!("{}_events", spindle_kernel::postgres_prefix(component));
    sql_value(
        url,
        &format!(
            "SELECT coalesce(max(t.seq_tup_read), 0) + coalesce(sum(i.idx_tup_read), 0) FROM \
             pg_stat_user_tables t LEFT JOIN pg_stat_user_indexes i ON i.relid = t.relid \
             WHERE t.schemaname = '{schema}' AND t.relname = '{table}'"
        ),
    )
    .parse()
    .expect("a count")
}
