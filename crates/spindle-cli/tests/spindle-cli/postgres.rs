//! `store.durable` bound to `postgres` (story postgres-store): `run` keeps rally's state in
//! PostgreSQL across a restart, `store migrate` creates every component's tables, and `run`
//! refuses before listening what eventlog would not admit.
//!
//! The refusals made before any connection run everywhere. The cases that reach a database are
//! ignored without `SPINDLE_TEST_DATABASE_URL`: a loopback PostgreSQL URL with user and password
//! (`postgres://owner:secret@127.0.0.1:5432/spindle`) of a role that may create schemas, roles and
//! tables. The hosted cases also need `SPINDLE_TEST_DATABASE_CA`, the PEM CA that server's TLS
//! certificate is issued by, for the host the URL names. `task check:postgres` runs them all.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const RALLY: &str = "examples/rally/ess";
const POSTGRES: &str = "examples/environments/postgres.toml";
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
const SETTINGS: [&str; 5] = [
    "SPINDLE_SETTING_DATABASE_URL",
    "SPINDLE_SETTING_DATABASE_CA",
    "SPINDLE_SETTING_DATABASE_SCHEMA",
    "SPINDLE_SETTING_DATABASE_POOL",
    "SPINDLE_SETTING_DATABASE_CONNECTIONS",
];
/// A password the refusals must never print.
const PASSWORD: &str = "n0t-printed-anywhere";

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

fn run(settings: &[(&str, &str)]) -> Command {
    spindle(&["run", RALLY, "--environment", POSTGRES], settings)
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

/// What a run printed before it exited without listening.
struct Refused {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Starts `command` and waits for its first stdout line: the listening line, or the end of a run
/// that exited first.
fn started(mut command: Command) -> Result<Served, Refused> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let (errors, collected) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0; 4096];
        while let Ok(read) = stderr.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buffer[..read]).into_owned();
            eprint!("{chunk}");
            text.push_str(&chunk);
            let _ = errors.send(text.clone());
        }
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
    if let Some(address) = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
        .and_then(|address| address.parse().ok())
    {
        return Ok(Served { child, address });
    }
    let deadline = Instant::now() + STARTUP;
    let status = loop {
        if let Some(status) = child.try_wait().expect("the run is waited on") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("run printed {line:?} and neither listened nor exited");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    std::thread::sleep(Duration::from_millis(100));
    Err(Refused {
        status,
        stdout: line,
        stderr: collected.try_iter().last().unwrap_or_default(),
    })
}

fn listening(command: Command) -> Served {
    match started(command) {
        Ok(served) => served,
        Err(refused) => panic!(
            "run exited {} before listening: {}",
            refused.status, refused.stderr
        ),
    }
}

fn refused(command: Command) -> Refused {
    match started(command) {
        Ok(served) => panic!("run listened on {}", served.address),
        Err(refused) => refused,
    }
}

/// Asserts `refused` exited 1 with nothing on stdout, naming every one of `named` on stderr and
/// never the password.
fn exits_1_naming(refused: &Refused, named: &[&str]) {
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    assert!(
        refused.stdout.is_empty(),
        "nothing is printed on stdout: {}",
        refused.stdout
    );
    for name in named {
        assert!(
            refused.stderr.contains(name),
            "the refusal names {name}: {}",
            refused.stderr
        );
    }
    assert!(
        !refused.stderr.contains(PASSWORD),
        "the refusal prints the password: {}",
        refused.stderr
    );
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

/// The route of rally's `operation`, its path variables filled from `body`.
fn route(operation: &str, body: &Value) -> (String, String) {
    let ir = spindle_spec::load(&root().join(RALLY))
        .expect("rally compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["rally-table".to_owned()], None)
        .expect("rally's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
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

fn submit(served: &Served, operation: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(operation, body);
    call(served.address, &method, &path, Some(body))
}

fn match_state(served: &Served, match_id: &str) -> Value {
    let (method, path) = route("rally.table.MatchState", &json!({ "match_id": match_id }));
    let (status, body) = call(served.address, &method, &path, None);
    assert_eq!(status, 200, "MatchState of {match_id} is answered: {body}");
    body
}

fn owner_url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn ca() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_CA").expect(
        "SPINDLE_TEST_DATABASE_CA names the PEM CA of the test server's TLS certificate (task \
         check:postgres)",
    )
}

/// `url` with `user` and `password` in place of its own.
fn as_role(url: &str, user: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a URL with a scheme");
    let (_, location) = rest.split_once('@').expect("a URL with user and password");
    format!("{scheme}://{user}:{password}@{location}")
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

/// A name no earlier run used: `stem` and twelve letters.
fn fresh(stem: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos()
        ^ u128::from(std::process::id()) << 64;
    let mut value = nanos;
    let mut letters = String::new();
    for _ in 0..12 {
        letters.push(char::from(
            b'a' + u8::try_from(value % 26).expect("below 26"),
        ));
        value /= 26;
    }
    format!("{stem}_{letters}")
}

/// A schema of its own for one case.
fn schema() -> String {
    let schema = fresh("spindle_test");
    sql(&[format!("CREATE SCHEMA {schema}")]);
    schema
}

fn tables(schema: &str) -> Vec<String> {
    sql(&[format!(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = '{schema}' ORDER \
         BY table_name"
    )])
}

fn migrate(settings: &[(&str, &str)]) -> std::process::Output {
    spindle(
        &["store", "migrate", RALLY, "--environment", POSTGRES],
        settings,
    )
    .output()
    .expect("store migrate runs")
}

/// A role that may log in and run DML on `schema`'s tables, its connection limit `limit`.
fn serving_role(schema: &str, limit: u32) -> String {
    let role = fresh("spindle_serving");
    let database = sql(&["SELECT current_database()".to_owned()]).remove(0);
    sql(&[
        format!("CREATE ROLE {role} LOGIN PASSWORD '{PASSWORD}' CONNECTION LIMIT {limit}"),
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA {schema} TO {role}"),
        format!("GRANT USAGE ON ALL SEQUENCES IN SCHEMA {schema} TO {role}"),
        format!(
            "SELECT pg_advisory_xact_lock({}); GRANT TEMPORARY ON DATABASE {database} TO {role}",
            crate::DATABASE_GRANT_LOCK
        ),
    ]);
    role
}

#[test]
fn run_on_postgres_without_database_url_exits_1_before_listening() {
    let refused = refused(run(&[]));
    exits_1_naming(&refused, &["database_url", "SPINDLE_SETTING_DATABASE_URL"]);
}

#[test]
fn run_against_a_host_off_loopback_without_database_ca_exits_1_naming_it() {
    let url = format!("postgres://serving:{PASSWORD}@db.example.invalid:5432/rally");
    let refused = refused(run(&[("database_url", &url)]));
    exits_1_naming(&refused, &["database_ca"]);
}

#[test]
fn run_with_a_connection_budget_below_components_times_pool_exits_1_naming_both() {
    let url = format!("postgres://serving:{PASSWORD}@127.0.0.1:1/rally");
    let refused = refused(run(&[
        ("database_url", &url),
        ("database_connections", "12"),
    ]));
    exits_1_naming(
        &refused,
        &["database_connections", "12", "13", "database_pool"],
    );
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn state_outlives_a_restart_on_postgres() {
    let schema = schema();
    let url = owner_url();
    let settings = [("database_url", url.as_str()), ("database_schema", &schema)];
    let first = listening(run(&settings));
    let (status, answer) = submit(
        &first,
        "rally.table.OpenMatch",
        &json!({"match_id": "kept", "target_score": 3}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let before = match_state(&first, "kept");
    drop(first);
    let second = listening(run(&settings));
    assert_eq!(match_state(&second, "kept"), before);
    let names = tables(&schema);
    for component in ["rally_table", "rally_player_west", "rally_player_east"] {
        assert!(
            names.contains(&format!("spindle_{component}_events")),
            "{component} keeps its own tables: {names:?}"
        );
    }
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn store_migrate_twice_on_an_empty_database_exits_0_and_leaves_the_same_tables() {
    let schema = schema();
    let url = owner_url();
    let settings = [("database_url", url.as_str()), ("database_schema", &schema)];
    assert!(tables(&schema).is_empty());
    let first = migrate(&settings);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let created = tables(&schema);
    assert!(
        created.contains(&"spindle_rally_table_events".to_owned()),
        "{created:?}"
    );
    let second = migrate(&settings);
    assert_eq!(
        second.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(tables(&schema), created);
    assert_eq!(first.stdout, second.stdout);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn run_with_the_owner_role_on_a_hosted_database_exits_1_naming_the_ddl_right() {
    let schema = schema();
    let (url, ca) = (owner_url(), ca());
    let settings = [
        ("database_url", url.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
        ("database_connections", "100"),
    ];
    let migrated = migrate(&settings);
    assert_eq!(
        migrated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    let refused = refused(run(&settings));
    exits_1_naming(&refused, &["DDL", "CREATE on the schema", "SUPERUSER"]);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn run_serves_rally_with_a_dml_only_role_on_a_hosted_database() {
    let schema = schema();
    let (url, ca) = (owner_url(), ca());
    let migrated = migrate(&[
        ("database_url", url.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
    ]);
    assert_eq!(
        migrated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    let role = serving_role(&schema, 12);
    let serving = as_role(&url, &role, PASSWORD);
    let settings = [
        ("database_url", serving.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
        ("database_connections", "13"),
    ];
    let first = listening(run(&settings));
    let (status, answer) = submit(
        &first,
        "rally.table.OpenMatch",
        &json!({"match_id": "hosted", "target_score": 3}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let before = match_state(&first, "hosted");
    drop(first);
    let second = listening(run(&settings));
    assert_eq!(match_state(&second, "hosted"), before);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn run_with_a_serving_role_limit_above_components_times_pool_exits_1_naming_it() {
    let schema = schema();
    let (url, ca) = (owner_url(), ca());
    let migrated = migrate(&[
        ("database_url", url.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
    ]);
    assert_eq!(migrated.status.code(), Some(0));
    let role = serving_role(&schema, 13);
    let serving = as_role(&url, &role, PASSWORD);
    let refused = refused(run(&[
        ("database_url", serving.as_str()),
        ("database_ca", &ca),
        ("database_schema", &schema),
        ("database_connections", "100"),
    ]));
    exits_1_naming(&refused, &["CONNECTION LIMIT from 1 to 12"]);
}
