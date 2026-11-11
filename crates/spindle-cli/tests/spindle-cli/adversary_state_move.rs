//! Adversary cases for `state move` (story state-moves-to-postgres): the refusal of a non-empty
//! prefix while another write transaction is open on the server, state the directory holds that
//! PostgreSQL's JSONB cannot hold, and a log long enough that append groups straddle the feed's
//! pages of 1000.
//!
//! Every case reaches a database and is ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback
//! PostgreSQL URL of a role that may create schemas and tables. Each case drops its schema.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use spindle_kernel::{
    Change, Database, Event, PostgresStore, Removal, Row, SqliteStore, Store, Write,
};

const RALLY: &str = "examples/rally/ess";
const POSTGRES: &str = "examples/environments/postgres.toml";
const COMPONENTS: [&str; 3] = ["rally-player-east", "rally-player-west", "rally-table"];
const MATCH: &str = "rally.table.Match";
const SETTINGS: [&str; 5] = [
    "SPINDLE_SETTING_DATABASE_URL",
    "SPINDLE_SETTING_DATABASE_CA",
    "SPINDLE_SETTING_DATABASE_SCHEMA",
    "SPINDLE_SETTING_DATABASE_POOL",
    "SPINDLE_SETTING_DATABASE_CONNECTIONS",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str], settings: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command.current_dir(root()).args(arguments);
    for variable in SETTINGS {
        command.env_remove(variable);
    }
    for (name, value) in settings {
        command.env(format!("SPINDLE_SETTING_{}", name.to_uppercase()), value);
    }
    command
}

fn text(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn state_move(directory: &Path, settings: &[(&str, &str)]) -> Output {
    let from = directory.to_str().expect("a UTF-8 path");
    spindle(
        &[
            "state",
            "move",
            "--from",
            from,
            RALLY,
            "--environment",
            POSTGRES,
        ],
        settings,
    )
    .output()
    .expect("state move runs")
}

fn migrate(settings: &[(&str, &str)]) {
    let output = spindle(
        &["store", "migrate", RALLY, "--environment", POSTGRES],
        settings,
    )
    .output()
    .expect("store migrate runs");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
}

fn owner_url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// Runs `statements` as the owner on a connection of their own; answers the first column of the
/// last statement's rows.
fn sql(statements: &[String]) -> Vec<String> {
    let runtime = runtime();
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(&owner_url(), tokio_postgres::NoTls)
            .await
            .expect("the test database accepts its owner");
        let driver = tokio::spawn(connection);
        let mut answered = Vec::new();
        for statement in statements {
            answered = client
                .simple_query(statement)
                .await
                .unwrap_or_else(|error| panic!("{statement}: {error}"))
                .into_iter()
                .filter_map(|message| match message {
                    tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                    _ => None,
                })
                .collect();
        }
        drop(client);
        let _ = driver.await;
        answered
    })
}

/// A schema of its own, dropped with everything in it when the case ends, red or green.
struct Schema(String);

impl Schema {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
            ^ u128::from(std::process::id()) << 64;
        let mut value = nanos;
        let mut name = format!("spindle_adv_{tag}_");
        for _ in 0..10 {
            name.push(char::from(
                b'a' + u8::try_from(value % 26).expect("below 26"),
            ));
            value /= 26;
        }
        sql(&[format!("CREATE SCHEMA {name}")]);
        Self(name)
    }
}

impl Drop for Schema {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(|| sql(&[format!("DROP SCHEMA {} CASCADE", self.0)]));
    }
}

/// How many events `component`'s tables in `schema` hold: a plain count, which no feed
/// watermark hides.
fn events(schema: &str, component: &str) -> u64 {
    let table = format!("{}_events", spindle_kernel::postgres_prefix(component));
    sql(&[format!("SELECT count(*) FROM {schema}.{table}")])[0]
        .parse()
        .expect("a count")
}

fn database(schema: &str) -> Database {
    let mut database = Database::new(owner_url());
    database.schema = schema.to_owned();
    database
}

fn file(directory: &Path, component: &str) -> PathBuf {
    directory.join(format!("{component}.sqlite"))
}

fn change(identity: &Value, expected: Option<u64>, fields: BTreeMap<String, Value>) -> Change {
    Change {
        entity: MATCH.to_owned(),
        identity: identity.clone(),
        expected,
        row: Row {
            state: "Serving".to_owned(),
            fields,
        },
        events: vec![Event {
            name: "rally.table.MatchOpened".to_owned(),
            payload: serde_json::Map::from_iter([("match_id".to_owned(), identity.clone())]),
        }],
        scanned: Vec::new(),
    }
}

/// One row, `first`, in each component's file of `directory`.
fn one_row_each(directory: &Path) {
    for component in COMPONENTS {
        SqliteStore::open(&file(directory, component))
            .expect("the file opens")
            .commit(&change(&json!("first"), None, BTreeMap::new()))
            .expect("a row is written");
    }
}

/// A write transaction left open, as any other client of the server may hold one: a
/// connection that began a transaction and took a transaction id. Rolled back on drop.
struct OpenTransaction {
    runtime: tokio::runtime::Runtime,
    client: tokio_postgres::Client,
}

impl OpenTransaction {
    fn begin() -> Self {
        let runtime = runtime();
        let client = runtime.block_on(async {
            let (client, connection) = tokio_postgres::connect(&owner_url(), tokio_postgres::NoTls)
                .await
                .expect("the test database accepts its owner");
            tokio::spawn(connection);
            client
                .batch_execute("BEGIN; SELECT pg_current_xact_id();")
                .await
                .expect("a transaction begins and takes an id");
            client
        });
        Self { runtime, client }
    }
}

impl Drop for OpenTransaction {
    fn drop(&mut self) {
        let _ = self.runtime.block_on(self.client.batch_execute("ROLLBACK"));
    }
}

/// The story's refusal ("when any component's prefix already holds a stream, the move exits 1
/// before writing to any prefix") holds while another client of the server has a write
/// transaction open. The emptiness check reads the feed, which shows an event only once every
/// write transaction older than it has ended; a plain count sees the stream.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_non_empty_prefix_refuses_the_move_while_another_transaction_is_open() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_row_each(directory.path());
    let schema = Schema::new("open");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    let elsewhere = OpenTransaction::begin();
    PostgresStore::open(&database(&schema.0), "rally-table", 1)
        .expect("the store opens")
        .commit(&change(&json!("already-here"), None, BTreeMap::new()))
        .expect("a row is written");
    let held: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema.0, component))
        .collect();
    assert_eq!(held, [0, 0, 3], "rally-table's tables hold a stream");
    let refused = state_move(directory.path(), &settings);
    let after: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema.0, component))
        .collect();
    drop(elsewhere);
    assert_eq!(
        after,
        held,
        "nothing is written to any prefix: {}",
        text(&refused)
    );
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("already hold streams"),
        "the refusal names the held prefix: {}",
        text(&refused)
    );
}

/// A row the directory holds moves whatever text its fields carry: a string with U+0000 is a
/// valid JSON string that the SQLite store keeps and answers.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_row_holding_a_nul_character_moves_or_the_move_writes_nothing() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_row_each(directory.path());
    let nul = json!("before\u{0}after");
    let mut table = SqliteStore::open(&file(directory.path(), "rally-table")).expect("opens");
    table
        .commit(&change(
            &nul,
            None,
            BTreeMap::from([("note".to_owned(), nul.clone())]),
        ))
        .expect("the SQLite store keeps the row");
    let left = table
        .load(MATCH, &nul)
        .expect("the SQLite store loads")
        .expect("the row is held");
    drop(table);
    let schema = Schema::new("nul");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    let moved = state_move(directory.path(), &settings);
    let written: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema.0, component))
        .collect();
    if moved.status.code() == Some(0) {
        let arrived = PostgresStore::open(&database(&schema.0), "rally-table", 1)
            .expect("the store opens")
            .load(MATCH, &nul)
            .expect("the store loads");
        assert_eq!(arrived.as_ref(), Some(&left), "{}", text(&moved));
    } else {
        assert_eq!(
            written,
            [0, 0, 0],
            "a move that cannot complete writes nothing, or the next move refuses a partial \
             copy it can never finish: {}",
            text(&moved)
        );
    }
}

/// A log of several thousand events, its append groups (one of them over 1000 events) straddling
/// the feed's pages of 1000, a removal and a re-creation among them: every stream arrives at its
/// version and every row as it rests, and the entity's version is the same.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_long_log_with_groups_across_pages_arrives_whole() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_row_each(directory.path());
    let mut table = SqliteStore::open(&file(directory.path(), "rally-table")).expect("opens");
    let mut versions = BTreeMap::<String, u64>::new();
    for round in 0..300_u64 {
        let identities: Vec<String> = (0..7_u64)
            .map(|slot| format!("m{}", (round * 7 + slot) % 40))
            .collect();
        let writes: Vec<Write> = identities
            .iter()
            .map(|identity| {
                Write::Commit(change(
                    &json!(identity),
                    versions.get(identity).copied(),
                    BTreeMap::from([("round".to_owned(), json!(round))]),
                ))
            })
            .collect();
        table.commit_all(&writes).expect("a group is written");
        for identity in identities {
            let stored = table
                .load(MATCH, &json!(identity))
                .expect("loads")
                .expect("held");
            versions.insert(identity, stored.version);
        }
    }
    let gone = json!("m3");
    table
        .remove(&Removal {
            entity: MATCH.to_owned(),
            identity: gone.clone(),
            expected: versions["m3"],
            events: Vec::new(),
        })
        .expect("removed");
    table
        .commit(&change(&gone, None, BTreeMap::new()))
        .expect("re-created after its removal");
    let wide: Vec<Write> = (0..700_u64)
        .map(|slot| Write::Commit(change(&json!(format!("w{slot}")), None, BTreeMap::new())))
        .collect();
    table
        .commit_all(&wide)
        .expect("one group of over 1000 events");
    let before = table.scan(MATCH).expect("the directory scans");
    drop(table);
    let schema = Schema::new("long");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    let started = std::time::Instant::now();
    let moved = state_move(directory.path(), &settings);
    let took = started.elapsed();
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    assert!(
        took < std::time::Duration::from_secs(120),
        "the move took {took:?}"
    );
    let after = PostgresStore::open(&database(&schema.0), "rally-table", 1)
        .expect("the store opens")
        .scan(MATCH)
        .expect("the moved store scans");
    assert_eq!(after, before);
}

// Adversary pass 2 (story state-moves-to-postgres, revision 8): what a refused move leaves in the
// prefixes, and two moves into one empty database at once.

/// How many rows `component`'s table `suffix` in `schema` holds.
fn rows(schema: &str, component: &str, suffix: &str) -> u64 {
    let table = format!("{}_{suffix}", spindle_kernel::postgres_prefix(component));
    sql(&[format!("SELECT count(*) FROM {schema}.{table}")])[0]
        .parse()
        .expect("a count")
}

/// The acceptance's "exits 1 before writing to any prefix" and the command's "Exits 1, writing
/// nothing": a move refused because one prefix holds a stream leaves every prefix's tables as it
/// found them, the tenant identity table among them.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_refused_move_writes_no_row_into_any_prefix() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_row_each(directory.path());
    let schema = Schema::new("ident");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    PostgresStore::open(&database(&schema.0), "rally-table", 1)
        .expect("the store opens")
        .commit(&change(&json!("already-here"), None, BTreeMap::new()))
        .expect("a row is written");
    let held = || -> Vec<(u64, u64)> {
        COMPONENTS
            .iter()
            .map(|component| {
                (
                    rows(&schema.0, component, "events"),
                    rows(&schema.0, component, "identity"),
                )
            })
            .collect()
    };
    let before = held();
    let refused = state_move(directory.path(), &settings);
    let after = held();
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert_eq!(
        after,
        before,
        "(events, identity rows) per component: a refused move writes nothing into any \
         prefix: {}",
        text(&refused)
    );
}

/// Two moves of two directories started at once into one empty database. The emptiness check of
/// each passes before the other's first append, so both copy: the second directory's first group
/// (an entity the first directory does not hold) lands, its next group conflicts with the first
/// directory's, and the move that exits 0 leaves a database holding another directory's streams
/// besides its own. Made deterministic by holding the schema's state move lock until both moves
/// wait on it: by then neither has read the tables or written anything. (Pass 2 held rally-table's
/// publication gate shared, which the emptiness check of fix pass 1 took exclusively; the check
/// of fix pass 2 takes no gate, and the move's lock is taken before it.)
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn two_moves_at_once_into_one_empty_database_leave_one_directory_or_none() {
    let first = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let second = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    for component in COMPONENTS {
        drop(SqliteStore::open(&file(first.path(), component)).expect("the file opens"));
        drop(SqliteStore::open(&file(second.path(), component)).expect("the file opens"));
    }
    let mut rally = change(&json!("from-second"), None, BTreeMap::new());
    rally.entity = "rally.player.Rally".to_owned();
    SqliteStore::open(&file(second.path(), "rally-player-east"))
        .expect("the file opens")
        .commit(&rally)
        .expect("a row is written");
    for (directory, identity) in [(&first, "from-first"), (&second, "from-second")] {
        SqliteStore::open(&file(directory.path(), "rally-table"))
            .expect("the file opens")
            .commit(&change(&json!(identity), None, BTreeMap::new()))
            .expect("a row is written");
    }
    let schema = Schema::new("race");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    // A scan records each log's tenant identity, as `run`'s first scan does, so each emptiness
    // check takes the gate once: without it, the check records the identity between two
    // captures and the faster move can append before the slower one's second capture.
    for component in COMPONENTS {
        PostgresStore::open(&database(&schema.0), component, 1)
            .expect("the store opens")
            .scan(MATCH)
            .expect("the empty store scans");
    }
    let holder = spindle_kernel::StateLock::take(&database(&schema.0))
        .expect("the schema's lock is held");
    let waiting = format!(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND NOT granted AND objsubid \
         = 2 AND objid::bigint = (hashtext('{}')::bigint & 4294967295)",
        schema.0
    );
    let start = |directory: &Path| {
        spindle(
            &[
                "state",
                "move",
                "--from",
                directory.to_str().expect("a UTF-8 path"),
                RALLY,
                "--environment",
                POSTGRES,
            ],
            &settings,
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("state move starts")
    };
    let one = start(first.path());
    let two = start(second.path());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let waiting: i64 = sql(std::slice::from_ref(&waiting))[0]
            .parse()
            .expect("a count");
        if waiting >= 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "both moves wait on the schema's lock"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    holder.release().expect("the schema's lock is released");
    let one = one.wait_with_output().expect("the first move ends");
    let two = two.wait_with_output().expect("the second move ends");
    let landed: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema.0, component))
        .collect();
    let report = format!(
        "events per component {landed:?}\nfirst {}\nsecond {}",
        text(&one),
        text(&two)
    );
    for (moved, directory) in [(&one, [0, 0, 3]), (&two, [3, 0, 3])] {
        if moved.status.code() == Some(0) {
            assert_eq!(
                landed, directory,
                "a move that exits 0 leaves its directory's streams and no other: {report}"
            );
        }
    }
}

/// What the U+0000 pass must not refuse, and the numbers PostgreSQL's `jsonb` keeps as `numeric`:
/// an identity holding U+0000 reaches PostgreSQL only as the stream's id, JSON text with the
/// character escaped, and a row field holding the six characters `\u0000` is no U+0000 at all.
/// Both move, and every row scans as the directory answered it.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn an_escaped_or_identity_only_nul_and_extreme_numbers_move_and_scan_alike() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_row_each(directory.path());
    let mut table = SqliteStore::open(&file(directory.path(), "rally-table")).expect("opens");
    let mut quiet = change(&json!("a\u{0}b"), None, BTreeMap::new());
    quiet.events.clear();
    table
        .commit(&quiet)
        .expect("an identity holding U+0000 is kept");
    table
        .commit(&change(
            &json!("edges"),
            None,
            BTreeMap::from([
                ("escaped".to_owned(), json!("\\u0000")),
                ("largest".to_owned(), json!(f64::MAX)),
                ("smallest".to_owned(), json!(5e-324_f64)),
                ("negative_zero".to_owned(), json!(-0.0_f64)),
                ("unsigned".to_owned(), json!(u64::MAX)),
                ("signed".to_owned(), json!(i64::MIN)),
                ("astral".to_owned(), json!("\u{1F600}\u{FFFF}\u{7F}\u{1}")),
            ]),
        ))
        .expect("the row is kept");
    let before = table.scan(MATCH).expect("the directory scans");
    drop(table);
    let schema = Schema::new("edge");
    let url = owner_url();
    let settings = [
        ("database_url", url.as_str()),
        ("database_schema", &schema.0),
    ];
    migrate(&settings);
    let moved = state_move(directory.path(), &settings);
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    let after = PostgresStore::open(&database(&schema.0), "rally-table", 1)
        .expect("the store opens")
        .scan(MATCH)
        .expect("the moved store scans");
    assert_eq!(after, before);
}
