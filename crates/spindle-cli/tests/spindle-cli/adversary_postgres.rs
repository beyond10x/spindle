//! Adversary pass 1, wave 19 (story postgres-store): `run` on `postgres`, its settings refusals,
//! and a served run on a database server other work shares.
//!
//! The settings cases run everywhere. The served case is ignored without
//! `SPINDLE_TEST_DATABASE_URL`: a loopback PostgreSQL whose role may create schemas and may
//! connect to the database `postgres` of the same server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const RALLY: &str = "examples/rally/ess";
const POSTGRES: &str = "examples/environments/postgres.toml";
const SETTINGS: [&str; 5] = [
    "SPINDLE_SETTING_DATABASE_URL",
    "SPINDLE_SETTING_DATABASE_CA",
    "SPINDLE_SETTING_DATABASE_SCHEMA",
    "SPINDLE_SETTING_DATABASE_POOL",
    "SPINDLE_SETTING_DATABASE_CONNECTIONS",
];
const PASSWORD: &str = "adv-n0t-printed";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(arguments: &[&str], settings: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", RALLY, "--environment", POSTGRES])
        .args(arguments);
    for variable in SETTINGS {
        command.env_remove(variable);
    }
    for (name, value) in settings {
        command.env(format!("SPINDLE_SETTING_{}", name.to_uppercase()), value);
    }
    command
}

/// Runs `command` to its end, or kills it after 60 s: (exit code, stdout, stderr).
fn ended(mut command: Command) -> (Option<i32>, String, String) {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let id = child.id();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    let output = receiver
        .recv_timeout(Duration::from_secs(60))
        .unwrap_or_else(|_| {
            let _ = Command::new("kill").arg(id.to_string()).status();
            panic!("run neither listened nor exited within 60 s")
        })
        .expect("the run is waited on");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A setting `run` cannot use is refused before anything listens, exit 1, naming the setting and
/// nothing misleading, and never the password.
#[test]
fn adv_a_malformed_setting_exits_1_naming_it_and_never_the_password() {
    let loopback = format!("postgres://serving:{PASSWORD}@127.0.0.1:1/rally");
    let cases: [(&[(&str, &str)], &str); 5] = [
        (&[("database_pool", "0")], "database_pool"),
        (&[("database_pool", "four")], "database_pool"),
        (&[("database_connections", "-3")], "database_connections"),
        (&[("database_schema", "Not A Schema")], "database_schema"),
        (
            &[("database_schema", "app; DROP TABLE x")],
            "database_schema",
        ),
    ];
    let mut failures = Vec::new();
    for (extra, named) in cases {
        let mut settings = vec![("database_url", loopback.as_str())];
        settings.extend_from_slice(extra);
        let (code, stdout, stderr) = ended(run(&["--answer-default"], &settings));
        if code != Some(1) || !stdout.trim().is_empty() {
            failures.push(format!("{extra:?}: exit {code:?}, stdout {stdout:?}"));
        }
        if !stderr.contains(named) {
            failures.push(format!("{extra:?}: does not name {named}: {stderr}"));
        }
        if stderr.contains("must name a loopback") {
            failures.push(format!(
                "{extra:?}: blames the loopback host 127.0.0.1: {stderr}"
            ));
        }
        if stderr.contains(PASSWORD) {
            failures.push(format!("{extra:?}: prints the password: {stderr}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
    let line = receiver
        .recv_timeout(Duration::from_secs(60))
        .unwrap_or_default();
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run did not listen: {line:?}"));
    Served { child, address }
}

fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(5)).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, text) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    (status, text.to_owned())
}

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

/// What `binary` serving rally on SQLite in `state` answers: each match's `MatchState`, and the
/// `spindle_entity_rows` lines of a scrape (a scan of every entity).
fn answers(binary: &Path, state: &Path, matches: &[&str], open: &[&str]) -> Vec<String> {
    let mut command = Command::new(binary);
    command.current_dir(root()).args([
        "run",
        RALLY,
        "--environment",
        "examples/environments/laptop.toml",
        "--observe",
        "--state",
    ]);
    command.arg(state);
    for variable in SETTINGS {
        command.env_remove(variable);
    }
    let served = listening(command);
    for match_id in open {
        let body = json!({"match_id": match_id, "target_score": 3});
        let (method, path) = route("rally.table.OpenMatch", &body);
        let (status, answer) = call(served.address, &method, &path, Some(&body));
        assert!(
            (200..300).contains(&status),
            "OpenMatch {match_id}: {status} {answer}"
        );
    }
    let mut seen = Vec::new();
    for match_id in matches {
        let (method, path) = route("rally.table.MatchState", &json!({"match_id": match_id}));
        let (status, body) = call(served.address, &method, &path, None);
        seen.push(format!("{match_id}: {status} {body}"));
    }
    let (status, body) = call(served.address, "GET", "/__spindle/metrics", None);
    seen.push(format!("metrics {status}"));
    seen.extend(
        body.lines()
            .filter(|line| line.starts_with("spindle_entity_rows{"))
            .map(str::to_owned),
    );
    seen
}

/// A state directory the base build (0.10.0's SQLite store, `SPINDLE_ADV_BASE_BIN`) wrote is
/// read by this build as the base build reads it, and takes new writes. Without
/// `SPINDLE_ADV_BASE_BIN` there is no base build to compare with, and the case says so and ends.
#[test]
#[ignore = "needs SPINDLE_ADV_BASE_BIN, a b10x-spindle built at the unit's base"]
fn adv_a_state_directory_the_base_build_wrote_answers_the_same() {
    let Some(base) = std::env::var_os("SPINDLE_ADV_BASE_BIN").map(PathBuf::from) else {
        eprintln!("skipped: SPINDLE_ADV_BASE_BIN names no base build");
        return;
    };
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let written = answers(
        &base,
        state.path(),
        &["one", "two", "none"],
        &["one", "two"],
    );
    let again = answers(&base, state.path(), &["one", "two", "none"], &[]);
    assert_eq!(again, written, "the base build reads its own state back");
    let read = answers(
        Path::new(env!("CARGO_BIN_EXE_b10x-spindle")),
        state.path(),
        &["one", "two", "none"],
        &[],
    );
    assert_eq!(
        read, written,
        "this build reads the base build's state as it does"
    );
    let extended = answers(
        Path::new(env!("CARGO_BIN_EXE_b10x-spindle")),
        state.path(),
        &["three"],
        &["three"],
    );
    assert!(extended[0].starts_with("three: 200"), "{extended:?}");
}

/// Adversary pass 3: the other way round. A state directory this build wrote (its
/// `spindle.written` events of schema 2, naming their instances) is read by the base build
/// (`SPINDLE_ADV_BASE_BIN`, 0.10.0) as this build reads it, takes the base build's writes, and is
/// read back by this build with both. Without `SPINDLE_ADV_BASE_BIN` the case says so and ends.
#[test]
#[ignore = "needs SPINDLE_ADV_BASE_BIN, a b10x-spindle built at the unit's base"]
fn adv3_a_state_directory_this_build_wrote_is_read_by_the_base_build() {
    let Some(base) = std::env::var_os("SPINDLE_ADV_BASE_BIN").map(PathBuf::from) else {
        eprintln!("skipped: SPINDLE_ADV_BASE_BIN names no base build");
        return;
    };
    let this = Path::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let written = answers(this, state.path(), &["one", "two", "none"], &["one", "two"]);
    let read = answers(&base, state.path(), &["one", "two", "none"], &[]);
    assert_eq!(
        read, written,
        "the base build reads this build's state as this build does"
    );
    let extended = answers(&base, state.path(), &["three"], &["three"]);
    assert!(extended[0].starts_with("three: 200"), "{extended:?}");
    let back = answers(
        this,
        state.path(),
        &["one", "two", "three", "none"],
        &["four"],
    );
    let again = answers(
        &base,
        state.path(),
        &["one", "two", "three", "four", "none"],
        &[],
    );
    let mine = answers(
        this,
        state.path(),
        &["one", "two", "three", "four", "none"],
        &[],
    );
    assert_eq!(
        again, mine,
        "both builds read the mixed state alike: {back:?}"
    );
}

fn owner_url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn on_database(url: &str, name: &str) -> String {
    let (head, _) = url.rsplit_once('/').expect("a URL with a database");
    format!("{head}/{name}")
}

fn batch(url: &str, statement: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
            .await
            .expect("the test database accepts its owner");
        let driver = tokio::spawn(connection);
        client
            .batch_execute(statement)
            .await
            .unwrap_or_else(|error| panic!("{statement}: {error}"));
        drop(client);
        let _ = driver.await;
    });
}

/// `url` with `user` and `password` in place of its own.
fn as_role(url: &str, user: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a URL with a scheme");
    let (_, location) = rest.split_once('@').expect("a URL with user and password");
    format!("{scheme}://{user}:{password}@{location}")
}

/// Rewritten in fix pass 1 of story:postgres-store, from
/// `adv_a_serving_role_whose_limit_is_the_budget_is_served_with_some_pool`: the story's
/// downstream setup gave `database_connections` the serving role's `CONNECTION LIMIT` (8 and 8),
/// which eventlog refuses at every pool. The documented rule is two numbers: the role's
/// `CONNECTION LIMIT` from 1 to components x `database_pool`, and `database_connections`, the
/// database's budget for the deployment, at least components x `database_pool` + 1.
///
/// The documented example, one component, `database_pool` 4, a role limited to 4 and
/// `database_connections` 8, is served. A role limited to the budget, 8, is refused naming the
/// `CONNECTION LIMIT` and `database_connections` as different numbers; a budget of 4 is refused
/// naming `database_connections` before anything connects.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv_the_documented_serving_role_limit_and_budget_are_served() {
    let url = owner_url();
    let ca = std::env::var("SPINDLE_TEST_DATABASE_CA").expect("SPINDLE_TEST_DATABASE_CA");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_nanos();
    let (schema, role) = (
        format!("adv1_w19_conn_{stamp}"),
        format!("adv1_w19_serving_{stamp}"),
    );
    batch(&url, &format!("CREATE SCHEMA {schema}"));
    let spec = "crates/spindle-cli/tests/fixtures/served-review";
    let mut migrate = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    migrate
        .current_dir(root())
        .args(["store", "migrate", spec, "--environment", POSTGRES])
        .env("SPINDLE_SETTING_DATABASE_URL", &url)
        .env("SPINDLE_SETTING_DATABASE_CA", &ca)
        .env("SPINDLE_SETTING_DATABASE_SCHEMA", &schema);
    let migrated = migrate.output().expect("store migrate runs");
    assert_eq!(
        migrated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&migrated.stderr)
    );
    batch(
        &url,
        &format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{PASSWORD}' CONNECTION LIMIT 4; GRANT USAGE ON \
             SCHEMA {schema} TO {role}; GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN \
             SCHEMA {schema} TO {role}; GRANT USAGE ON ALL SEQUENCES IN SCHEMA {schema} TO \
             {role}; GRANT TEMPORARY ON DATABASE spindle TO {role}"
        ),
    );
    let serving = as_role(&url, &role, PASSWORD);
    // Ok(the first line on stdout) where run listened, else Err((its exit, its refusal)).
    let attempt = |connections: &str| -> Result<String, (Option<i32>, String)> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .current_dir(root())
            .args(["run", spec, "--environment", POSTGRES, "--answer-default"])
            .env("SPINDLE_SETTING_DATABASE_URL", &serving)
            .env("SPINDLE_SETTING_DATABASE_CA", &ca)
            .env("SPINDLE_SETTING_DATABASE_SCHEMA", &schema)
            .env("SPINDLE_SETTING_DATABASE_CONNECTIONS", connections)
            .env("SPINDLE_SETTING_DATABASE_POOL", "4");
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let stdout = child.stdout.take().expect("stdout");
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        if line.contains("listening") {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(line);
        }
        let output = child.wait_with_output().expect("the run ends");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let reason = stderr
            .lines()
            .find(|line| line.starts_with("b10x-spindle:"))
            .unwrap_or(&stderr)
            .to_owned();
        Err((output.status.code(), reason))
    };
    let documented = attempt("8");
    let small_budget = attempt("4");
    batch(&url, &format!("ALTER ROLE {role} CONNECTION LIMIT 8"));
    let limit_is_budget = attempt("8");
    batch(
        &url,
        &format!("DROP SCHEMA {schema} CASCADE; DROP OWNED BY {role}; DROP ROLE {role}"),
    );

    let line = documented.unwrap_or_else(|refused| {
        panic!("role limit 4, pool 4, database_connections 8 is refused: {refused:?}")
    });
    assert!(line.contains("listening"), "{line}");

    let (code, reason) = small_budget.expect_err("database_connections 4 at pool 4 is refused");
    assert_eq!(code, Some(1), "{reason}");
    for named in ["database_connections is 4", "5", "CONNECTION LIMIT"] {
        assert!(reason.contains(named), "does not name {named}: {reason}");
    }

    let (code, reason) = limit_is_budget.expect_err("a role limited to the budget, 8, is refused");
    assert_eq!(code, Some(1), "{reason}");
    for named in [
        "CONNECTION LIMIT from 1 to 4",
        "database_connections",
        "at least 5",
    ] {
        assert!(reason.contains(named), "does not name {named}: {reason}");
    }
    assert!(!reason.contains(PASSWORD), "prints the password: {reason}");
}

/// A write transaction open in another database of the server, and the sender that ends it.
fn hold_a_write_transaction(url: String) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (opened, ready) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let (mut client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
                .await
                .expect("a connection to the other database");
            let driver = tokio::spawn(connection);
            let transaction = client.transaction().await.expect("a transaction");
            transaction
                .simple_query("SELECT pg_current_xact_id()")
                .await
                .expect("the transaction takes an id");
            opened.send(()).expect("the test waits");
            let _ = tokio::task::spawn_blocking(move || {
                let _ = released.recv_timeout(Duration::from_secs(60));
            })
            .await;
            transaction.rollback().await.expect("the transaction ends");
            drop(client);
            let _ = driver.await;
        });
    });
    ready.recv().expect("the write transaction is open");
    (release, holder)
}

/// With a write transaction open in another database of the server, a served rally still
/// answers: a metrics scrape (which scans every entity) answers 200, and a `MatchState` asked
/// while the scrape is in flight is answered within 3 s.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_a_served_run_answers_while_another_database_holds_a_write_transaction() {
    let url = owner_url();
    let schema = format!(
        "adv1_w19_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after 1970")
            .as_nanos()
    );
    batch(&url, &format!("CREATE SCHEMA {schema}"));
    let served = listening(run(
        &["--observe"],
        &[("database_url", url.as_str()), ("database_schema", &schema)],
    ));
    let (release, holder) = hold_a_write_transaction(on_database(&url, "postgres"));
    let opened = json!({"match_id": "held", "target_score": 3});
    let (method, path) = route("rally.table.OpenMatch", &opened);
    let started = Instant::now();
    let (status, answer) = call(served.address, &method, &path, Some(&opened));
    let open_took = started.elapsed();
    let address = served.address;
    let scrape = std::thread::spawn(move || {
        let started = Instant::now();
        let (status, body) = call(address, "GET", "/__spindle/metrics", None);
        (status, body, started.elapsed())
    });
    std::thread::sleep(Duration::from_millis(300));
    let (method, state_path) = route("rally.table.MatchState", &json!({"match_id": "held"}));
    let started = Instant::now();
    let (state_status, state) = call(served.address, &method, &state_path, None);
    let state_took = started.elapsed();
    let (scrape_status, scrape_body, scrape_took) = scrape.join().expect("the scrape");
    let _ = release.send(());
    holder.join().expect("the other transaction ends");
    let report = format!(
        "OpenMatch {status} in {open_took:?}: {answer}\nmetrics {scrape_status} in \
         {scrape_took:?}: {}\nMatchState {state_status} in {state_took:?}: {state}",
        scrape_body.chars().take(400).collect::<String>()
    );
    assert!((200..300).contains(&status), "{report}");
    assert_eq!(scrape_status, 200, "{report}");
    assert!(state_took < Duration::from_secs(3), "{report}");
}
