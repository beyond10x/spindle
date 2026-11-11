//! `state move` (story state-moves-to-postgres): a `--state` directory's streams move into the
//! PostgreSQL store, versions kept, and `run` on PostgreSQL answers as the run on the directory
//! did.
//!
//! The refusal made before any connection runs everywhere. The cases that reach a database are
//! ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback PostgreSQL URL with user and password
//! of a role that may create schemas and tables; `task check:postgres` runs them.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_kernel::{Change, Database, Event, PostgresStore, Row, SqliteStore, Store, StoreError};

const RALLY: &str = "examples/rally/ess";
const LAPTOP: &str = "examples/environments/laptop.toml";
const POSTGRES: &str = "examples/environments/postgres.toml";
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
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

/// `b10x-spindle` with `arguments`, every database setting cleared and then `settings` set.
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

/// `state move --from <directory>` of rally into the database `settings` name.
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

struct Served {
    child: Child,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts `command` and waits for its listening line.
fn listening(mut command: Command) -> Served {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| {
            let _ = child.kill();
            panic!("run printed {line:?} and did not listen")
        });
    Served { child, address }
}

fn on_directory(directory: &Path) -> Served {
    let state = directory.to_str().expect("a UTF-8 path");
    listening(spindle(
        &["run", RALLY, "--environment", LAPTOP, "--state", state],
        &[],
    ))
}

fn on_postgres(settings: &[(&str, &str)]) -> Served {
    listening(spindle(
        &["run", RALLY, "--environment", POSTGRES],
        settings,
    ))
}

fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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

/// Every route `run` serves for rally.
fn routes() -> Vec<spindle_ingress::Route> {
    let ir = spindle_spec::load(&root().join(RALLY))
        .expect("rally compiles")
        .ir;
    spindle_ingress::Surface::new(&ir, &["rally-table".to_owned()], None)
        .expect("rally's surface is routed")
        .routes()
        .to_vec()
}

/// `route`'s method and path, its path variables filled from `body`.
fn filled(route: &spindle_ingress::Route, body: &Value) -> (String, String) {
    let path = route
        .variables()
        .iter()
        .fold(route.path.clone(), |path, variable| {
            let value = match &body[variable.field.as_str()] {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            path.replace(&format!("{{{}}}", variable.field), &value)
        });
    (route.method.as_str().to_uppercase(), path)
}

fn submit(served: &Served, operation: &str, body: &Value) -> Value {
    let route = routes()
        .into_iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    let (method, path) = filled(&route, body);
    let (status, answer) = call(served.address, &method, &path, Some(body));
    assert!(
        (200..300).contains(&status),
        "{operation} {body}: {status} {answer}"
    );
    answer
}

/// What every view `run` serves answers for each of `matches`, by view and match.
fn views(served: &Served, matches: &[&str]) -> BTreeMap<String, (u16, Value)> {
    let mut answered = BTreeMap::new();
    for route in routes()
        .iter()
        .filter(|route| route.method.as_str().eq_ignore_ascii_case("get"))
    {
        for match_id in matches {
            let (method, path) = filled(route, &json!({ "match_id": match_id }));
            answered.insert(
                format!("{} {path}", route.operation.name()),
                call(served.address, &method, &path, None),
            );
        }
    }
    assert!(
        answered
            .keys()
            .any(|key| key.starts_with("rally.table.MatchState ")),
        "MatchState is among the views: {answered:?}"
    );
    answered
}

/// The commands the directory holds before the move: two matches, paddles moved, balls ticked.
fn play(served: &Served) {
    for (match_id, target) in [("first", 3), ("second", 5)] {
        submit(
            served,
            "rally.table.OpenMatch",
            &json!({"match_id": match_id, "target_score": target}),
        );
    }
    submit(
        served,
        "rally.table.MovePaddle",
        &json!({"match_id": "first", "side": "West", "step": "Up"}),
    );
    for _ in 0..3 {
        submit(served, "rally.table.Tick", &json!({"match_id": "first"}));
    }
    submit(
        served,
        "rally.table.MovePaddle",
        &json!({"match_id": "second", "side": "East", "step": "Down"}),
    );
}

fn owner_url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

/// Runs `statements` as the test database's owner, and answers the first column of the last
/// query's rows.
fn sql(statements: &[String]) -> Vec<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
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

/// A schema of its own for one case: `spindle_move_` and twelve letters.
fn schema() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos()
        ^ u128::from(std::process::id()) << 64;
    let mut value = nanos;
    let mut schema = "spindle_move_".to_owned();
    for _ in 0..12 {
        schema.push(char::from(
            b'a' + u8::try_from(value % 26).expect("below 26"),
        ));
        value /= 26;
    }
    sql(&[format!("CREATE SCHEMA {schema}")]);
    schema
}

/// How many events `component`'s tables in `schema` hold.
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

/// Every entity's scan in each component's store, by component and entity.
fn scans(open: &mut dyn FnMut(&str) -> Box<dyn Store>) -> BTreeMap<String, spindle_kernel::Scan> {
    let ir = spindle_spec::load(&root().join(RALLY))
        .expect("rally compiles")
        .ir;
    let mut scans = BTreeMap::new();
    for component in COMPONENTS {
        let mut store = open(component);
        for entity in ir.entities().keys() {
            let entity = entity.to_string();
            let scan = store.scan(&entity).expect("the store scans");
            scans.insert(format!("{component} {entity}"), scan);
        }
    }
    scans
}

fn settings<'a>(url: &'a str, schema: &'a str) -> [(&'a str, &'a str); 2] {
    [("database_url", url), ("database_schema", schema)]
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn state_moves_into_postgres_and_answers_as_before() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let before = {
        let served = on_directory(directory.path());
        play(&served);
        views(&served, &["first", "second", "absent"])
    };
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    let moved = state_move(directory.path(), &settings);
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    let lines: Vec<Value> = String::from_utf8_lossy(&moved.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    let named: Vec<&str> = lines
        .iter()
        .map(|line| line["component"].as_str().expect("a component"))
        .collect();
    assert_eq!(named, COMPONENTS, "{}", text(&moved));
    for line in &lines {
        let component = line["component"].as_str().expect("a component");
        assert!(line["streams"].as_u64() > Some(0), "{line}");
        assert_eq!(
            line["events"].as_u64(),
            Some(events(&schema, component)),
            "{line}"
        );
    }
    let mut from_files = scans(&mut |component| {
        Box::new(
            SqliteStore::open(&directory.path().join(format!("{component}.sqlite")))
                .expect("the directory's store opens"),
        )
    });
    let database = database(&schema);
    let mut from_postgres = scans(&mut |component| {
        Box::new(PostgresStore::open(&database, component, 1).expect("the moved store opens"))
    });
    assert_eq!(from_postgres.len(), from_files.len());
    for (key, scan) in &mut from_files {
        assert_eq!(from_postgres.remove(key).as_ref(), Some(&*scan), "{key}");
    }
    let served = on_postgres(&settings);
    assert_eq!(views(&served, &["first", "second", "absent"]), before);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_moved_instance_continues_at_its_version() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    {
        let served = on_directory(directory.path());
        play(&served);
    }
    let left = SqliteStore::open(&directory.path().join("rally-table.sqlite"))
        .expect("the directory's store opens")
        .load(MATCH, &json!("first"))
        .expect("the directory's store loads")
        .expect("the match is held");
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    let moved = state_move(directory.path(), &settings);
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    let database = database(&schema);
    let arrived = PostgresStore::open(&database, "rally-table", 1)
        .expect("the store opens")
        .load(MATCH, &json!("first"))
        .expect("the store loads");
    assert_eq!(
        arrived.as_ref(),
        Some(&left),
        "the match arrives at its version"
    );
    {
        let served = on_postgres(&settings);
        submit(
            &served,
            "rally.table.MovePaddle",
            &json!({"match_id": "first", "side": "East", "step": "Up"}),
        );
    }
    let mut store = PostgresStore::open(&database, "rally-table", 1).expect("the store opens");
    let now = store
        .load(MATCH, &json!("first"))
        .expect("the store loads")
        .expect("the match is held");
    // A version counts the stream's events: the commit names the version the directory left and
    // appends at the one after it the command's event (PaddleMoved) and the row (spindle.row).
    assert_eq!(now.version, left.version + 2, "{left:?} then {now:?}");
    let stale = Change {
        entity: MATCH.to_owned(),
        identity: json!("first"),
        expected: Some(left.version),
        row: Row {
            state: now.row.state.clone(),
            fields: now.row.fields.clone(),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    };
    match store.commit(&stale) {
        Err(StoreError::Conflict {
            expected, actual, ..
        }) => {
            assert_eq!(expected, Some(left.version));
            assert_eq!(actual, Some(now.version));
        }
        other => panic!("a commit naming the old version is refused as a conflict: {other:?}"),
    }
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_move_into_a_non_empty_prefix_writes_nothing() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    {
        let served = on_directory(directory.path());
        play(&served);
    }
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    let database = database(&schema);
    PostgresStore::open(&database, "rally-table", 1)
        .expect("the store opens")
        .commit(&Change {
            entity: MATCH.to_owned(),
            identity: json!("already-here"),
            expected: None,
            row: Row {
                state: "Serving".to_owned(),
                fields: BTreeMap::new(),
            },
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("a row is written");
    let held: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    assert_eq!(held, [0, 0, 2]);
    let refused = state_move(directory.path(), &settings);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(refused.stdout.is_empty(), "{}", text(&refused));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("rally-table"),
        "the refusal names the component: {}",
        text(&refused)
    );
    let after: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    assert_eq!(after, held, "nothing is written to any prefix");
}

#[test]
fn a_move_from_a_directory_without_the_components_files_exits_1_naming_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let refused = state_move(
        directory.path(),
        &[("database_url", "postgres://owner:secret@127.0.0.1:1/rally")],
    );
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(refused.stdout.is_empty(), "{}", text(&refused));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains(directory.path().to_str().expect("a UTF-8 path")),
        "the refusal names the directory: {}",
        text(&refused)
    );
    assert!(
        stderr.contains("rally-table.sqlite"),
        "the refusal names the missing file: {}",
        text(&refused)
    );
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("the directory is read")
            .count(),
        0,
        "the move creates nothing in the directory"
    );
}

/// A JSON string holding U+0000, which the SQLite store keeps and PostgreSQL refuses, here in an
/// object key of an event: the move reads every file before it connects, and exits 1 naming the
/// component and the stream, nothing written.
#[test]
fn a_move_of_a_string_postgresql_refuses_exits_1_before_connecting() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    for component in COMPONENTS {
        let mut store = SqliteStore::open(&directory.path().join(format!("{component}.sqlite")))
            .expect("the file opens");
        store
            .commit(&Change {
                entity: MATCH.to_owned(),
                identity: json!("first"),
                expected: None,
                row: Row {
                    state: "Serving".to_owned(),
                    fields: BTreeMap::new(),
                },
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("a row is written");
    }
    SqliteStore::open(&directory.path().join("rally-table.sqlite"))
        .expect("the file opens")
        .commit(&Change {
            entity: MATCH.to_owned(),
            identity: json!("held"),
            expected: None,
            row: Row {
                state: "Serving".to_owned(),
                fields: BTreeMap::new(),
            },
            events: vec![Event {
                name: "rally.table.MatchOpened".to_owned(),
                payload: serde_json::Map::from_iter([(
                    "detail".to_owned(),
                    json!([{"no\u{0}te": 1}]),
                )]),
            }],
            scanned: Vec::new(),
        })
        .expect("the SQLite store keeps the event");
    let refused = state_move(
        directory.path(),
        &[("database_url", "postgres://owner:secret@127.0.0.1:1/rally")],
    );
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(refused.stdout.is_empty(), "{}", text(&refused));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    for named in [
        "rally-table",
        r#"rally.table.Match "held""#,
        "U+0000",
        r#"["detail"][0]["no\u0000te"]"#,
        "nothing was moved",
    ] {
        assert!(
            stderr.contains(named),
            "the refusal names {named}: {}",
            text(&refused)
        );
    }
    assert!(
        !stderr.contains("127.0.0.1"),
        "the refusal comes before any connection: {}",
        text(&refused)
    );
}

/// A directory of rally's three files, each holding one match.
fn one_match_each(directory: &Path) {
    for component in COMPONENTS {
        SqliteStore::open(&directory.join(format!("{component}.sqlite")))
            .expect("the file opens")
            .commit(&Change {
                entity: MATCH.to_owned(),
                identity: json!("first"),
                expected: None,
                row: Row {
                    state: "Serving".to_owned(),
                    fields: BTreeMap::new(),
                },
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("a row is written");
    }
}

/// While another move holds the schema, a move waits a little, then exits 1 naming the other
/// move and the schema, writing nothing; once that move ends, the same move succeeds.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_move_into_a_schema_another_move_holds_exits_1_naming_it_and_writes_nothing() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_match_each(directory.path());
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    let other =
        spindle_kernel::StateLock::take(&database(&schema)).expect("another move takes the schema");
    let refused = state_move(directory.path(), &settings);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(refused.stdout.is_empty(), "{}", text(&refused));
    let stderr = String::from_utf8_lossy(&refused.stderr);
    for named in ["another state move", schema.as_str(), "nothing was moved"] {
        assert!(
            stderr.contains(named),
            "the refusal names {named}: {}",
            text(&refused)
        );
    }
    let held: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    assert_eq!(held, [0, 0, 0], "nothing is written to any prefix");
    other.release().expect("the other move ends");
    let moved = state_move(directory.path(), &settings);
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
}

/// The tables of a component hold a row of an entity the component declares and the directory
/// does not hold: the move exits 1 naming the component, nothing written.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_move_into_tables_holding_an_entity_the_directory_does_not_exits_1() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_match_each(directory.path());
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    PostgresStore::open(&database(&schema), "rally-player-east", 1)
        .expect("the store opens")
        .commit(&Change {
            entity: "rally.east.Player".to_owned(),
            identity: json!("already-here"),
            expected: None,
            row: Row {
                state: "Serving".to_owned(),
                fields: BTreeMap::new(),
            },
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("a row is written");
    let refused = state_move(directory.path(), &settings);
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("rally-player-east"),
        "the refusal names the component: {}",
        text(&refused)
    );
    let after: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    assert_eq!(after, [2, 0, 0], "nothing is written to any prefix");
}

const PASSWORD: &str = "serving-password";

/// A role that may log in and run DML on `schema`'s tables, as `run` serves with on a hosted
/// database.
fn serving_role(schema: &str) -> String {
    let role = format!("spindle_move_serving_{}", &schema["spindle_move_".len()..]);
    let database = sql(&["SELECT current_database()".to_owned()]).remove(0);
    sql(&[
        format!("CREATE ROLE {role} LOGIN PASSWORD '{PASSWORD}' CONNECTION LIMIT 12"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA {schema} TO {role}"),
        format!("GRANT USAGE ON ALL SEQUENCES IN SCHEMA {schema} TO {role}"),
        format!("GRANT TEMPORARY ON DATABASE {database} TO {role}"),
    ]);
    role
}

/// On a hosted database, over TLS, the serving role `run` takes moves the directory: it takes
/// its schema's lock as any role may.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn a_move_with_the_serving_role_on_a_hosted_database_moves_the_directory() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    one_match_each(directory.path());
    let schema = schema();
    let url = owner_url();
    let ca = std::env::var("SPINDLE_TEST_DATABASE_CA").expect(
        "SPINDLE_TEST_DATABASE_CA names the PEM CA of the test server's TLS certificate (task \
         check:postgres)",
    );
    migrate(&[
        ("database_url", url.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
    ]);
    let role = serving_role(&schema);
    let (scheme, rest) = url.split_once("://").expect("a URL with a scheme");
    let (_, location) = rest.split_once('@').expect("a URL with user and password");
    let serving = format!("{scheme}://{role}:{PASSWORD}@{location}");
    let moved = state_move(
        directory.path(),
        &[
            ("database_url", serving.as_str()),
            ("database_ca", &ca),
            ("database_schema", &schema),
            ("database_connections", "13"),
        ],
    );
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    let landed: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    assert_eq!(landed, [2, 2, 2], "{}", text(&moved));
}

/// Two moves of two directories started together into one empty schema, both waiting on its
/// lock before either has read the tables: one moves its directory, the other exits 1, and the
/// schema holds one directory's streams, never a mix.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn two_moves_started_together_into_one_schema_leave_one_directory() {
    let first = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let second = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let row = |entity: &str, identity: &str| Change {
        entity: entity.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "Serving".to_owned(),
            fields: BTreeMap::new(),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    };
    for directory in [first.path(), second.path()] {
        for component in COMPONENTS {
            drop(
                SqliteStore::open(&directory.join(format!("{component}.sqlite")))
                    .expect("the file opens"),
            );
        }
    }
    let write = |directory: &Path, component: &str, change: &Change| {
        SqliteStore::open(&directory.join(format!("{component}.sqlite")))
            .expect("the file opens")
            .commit(change)
            .expect("a row is written");
    };
    write(first.path(), "rally-table", &row(MATCH, "from-first"));
    write(
        second.path(),
        "rally-player-east",
        &row("rally.east.Player", "from-second"),
    );
    write(second.path(), "rally-table", &row(MATCH, "from-second"));
    let schema = schema();
    let url = owner_url();
    let settings = settings(&url, &schema);
    migrate(&settings);
    let held = spindle_kernel::StateLock::take(&database(&schema)).expect("the schema is held");
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("state move starts")
    };
    let one = start(first.path());
    let two = start(second.path());
    let waiting = format!(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 2 AND NOT \
         granted AND objid::bigint = (hashtext('{schema}')::bigint & 4294967295)"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    while sql(std::slice::from_ref(&waiting))[0] != "2" {
        assert!(
            std::time::Instant::now() < deadline,
            "both moves wait on the schema's lock"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    held.release().expect("the schema is released");
    let one = one.wait_with_output().expect("the first move ends");
    let two = two.wait_with_output().expect("the second move ends");
    let landed: Vec<u64> = COMPONENTS
        .iter()
        .map(|component| events(&schema, component))
        .collect();
    let report = format!(
        "events per component {landed:?}\nfirst {}\nsecond {}",
        text(&one),
        text(&two)
    );
    let exited = [one.status.code(), two.status.code()];
    assert!(
        exited.iter().all(|code| matches!(code, Some(0 | 1)))
            && exited.iter().filter(|code| **code == Some(0)).count() <= 1,
        "at most one move moves, the other exits 1: {report}"
    );
    // The second waits on the lock while the first moves: it is refused for the tables the
    // first filled, or for the lock it still holds after the wait.
    let expected = match exited {
        [Some(0), _] => [0, 0, 2],
        [_, Some(0)] => [2, 0, 2],
        _ => [0, 0, 0],
    };
    assert_eq!(
        landed, expected,
        "the schema holds the directory of the move that exited 0, or nothing: {report}"
    );
}
