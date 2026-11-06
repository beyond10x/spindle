//! `run` serving HTTP with several fixed actors (`--fixed-actor` given more than once, each with
//! its own `--fixed-caller`): a test-backend switch, not authentication. A served command is sent
//! as the first fixed actor whose `may:` names it, a read-granted view read as the first that may
//! read it; an operation no fixed actor may run answers ESS's standard `403`, naming the first.
//!
//! The fixture `tests/fixtures/fixed-callers` declares one component, `office`, reached over the
//! network. `Clerk` carries `badge` and `shift` and may open and note a desk; `Keeper` carries
//! `key` and `shift`, may lock and note a desk and may read `DeskLedger`, which makes that view
//! read-granted; `Warden` alone may reserve a desk. `OpenDesk` records the caller's `badge` and
//! refuses with `own-name` a desk named after it; `LockDesk` records the caller's `key` and refuses
//! with `wrong-key` any other key; `NoteDesk` records the caller's `shift`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/fixed-callers";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "office";
const CLERK: &str = "bureau.desk.Clerk";
const KEEPER: &str = "bureau.desk.Keeper";
const OPEN: &str = "bureau.desk.OpenDesk";
const LOCK: &str = "bureau.desk.LockDesk";
const NOTE: &str = "bureau.desk.NoteDesk";
const RESERVE: &str = "bureau.desk.ReserveDesk";
const CARD: &str = "bureau.desk.DeskCard";
const LEDGER: &str = "bureau.desk.DeskLedger";
const DESK: &str = "7d1c2b3a-4e5f-4a6b-9c8d-0e1f2a3b4c5d";
const OTHER_DESK: &str = "8e2d3c4b-5f6a-4b7c-8d9e-1f2a3b4c5d6e";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The method and path the served surface puts `operation` at.
fn route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the fixed-callers fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: mpsc::Receiver<String>,
}

impl Served {
    /// Stops the run and answers everything it printed on stderr.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end once the run stops")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on the fixture and laptop with `arguments` added; `Err` holds the exit code, stdout and
/// stderr of a run that stopped without listening.
fn serve(arguments: &[&str]) -> Result<Served, (Option<i32>, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (printed, stderr_read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = printed.send(text);
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let mut rest = String::new();
        if line.is_empty() {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
    });
    let Ok((line, rest)) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let stderr = stderr_read
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end");
        return Err((status.code(), rest, stderr));
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served {
        child,
        address,
        stderr: stderr_read,
    })
}

fn serving(arguments: &[&str]) -> Served {
    serve(arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
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
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

fn call(served: &Served, operation: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(operation);
    exchange(served.address, &method, &path, body)
}

fn clerk() -> String {
    json!({"badge": "ada", "shift": "early"}).to_string()
}

fn keeper() -> String {
    json!({"key": "brass", "shift": "late"}).to_string()
}

/// The arguments fixing the clerk, then the keeper.
fn clerk_then_keeper<'a>(clerk: &'a str, keeper: &'a str) -> [&'a str; 8] {
    [
        "--fixed-actor",
        CLERK,
        "--fixed-caller",
        clerk,
        "--fixed-actor",
        KEEPER,
        "--fixed-caller",
        keeper,
    ]
}

/// The arguments fixing the keeper, then the clerk.
fn keeper_then_clerk<'a>(clerk: &'a str, keeper: &'a str) -> [&'a str; 8] {
    [
        "--fixed-actor",
        KEEPER,
        "--fixed-caller",
        keeper,
        "--fixed-actor",
        CLERK,
        "--fixed-caller",
        clerk,
    ]
}

fn open(served: &Served, desk: &str, name: &str) -> (u16, Value) {
    call(served, OPEN, Some(&json!({"desk_id": desk, "name": name})))
}

#[test]
fn each_command_runs_as_the_fixed_actor_granted_it() {
    let (clerk, keeper) = (clerk(), keeper());
    let served = serving(&clerk_then_keeper(&clerk, &keeper));
    let (status, opened) = open(&served, DESK, "window");
    assert_eq!(status, 202, "{opened}");
    assert_eq!(opened["outcome"], "opened", "{opened}");
    assert_eq!(
        opened["published"][0]["payload"]["opened_by"], "ada",
        "OpenDesk is sent as the clerk, its only grantee: {opened}"
    );
    let (_, refused) = open(&served, OTHER_DESK, "ada");
    assert_eq!(
        refused["outcome"], "own-name",
        "the guard `name == caller.badge` reads the clerk's badge: {refused}"
    );
    let (_, refused) = call(
        &served,
        LOCK,
        Some(&json!({"desk_id": DESK, "with_key": "ada"})),
    );
    assert_eq!(
        refused["outcome"], "wrong-key",
        "the guard `with_key != caller.key` reads the keeper's key: {refused}"
    );
    let (status, locked) = call(
        &served,
        LOCK,
        Some(&json!({"desk_id": DESK, "with_key": "brass"})),
    );
    assert_eq!(status, 202, "{locked}");
    assert_eq!(locked["outcome"], "locked", "{locked}");
    assert_eq!(
        locked["published"][0]["payload"]["locked_by"], "brass",
        "LockDesk is sent as the keeper, its only grantee, though the clerk is given first: {locked}"
    );
    let (status, rows) = call(&served, CARD, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows["rows"][0]["opened_by"], "ada", "{rows}");
    assert_eq!(rows["rows"][0]["locked_by"], "brass", "{rows}");
}

#[test]
fn an_operation_no_fixed_actor_may_run_is_refused_403() {
    let (clerk, keeper) = (clerk(), keeper());
    let served = serving(&clerk_then_keeper(&clerk, &keeper));
    let (status, body) = call(
        &served,
        RESERVE,
        Some(&json!({"desk_id": DESK, "opened_by": "nobody"})),
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({"refused": "not granted", "actor": CLERK}),
        "ESS's standard refusal, naming the first fixed actor"
    );
    let (status, rows) = call(&served, CARD, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows["rows"], json!([]), "nothing is written: {rows}");
    let (status, events) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    assert_eq!(
        events["events"],
        json!([]),
        "nothing is published: {events}"
    );
}

#[test]
fn the_fixed_actors_are_printed_at_startup_in_order() {
    let (clerk, keeper) = (clerk(), keeper());
    for (arguments, order) in [
        (clerk_then_keeper(&clerk, &keeper), [CLERK, KEEPER]),
        (keeper_then_clerk(&clerk, &keeper), [KEEPER, CLERK]),
    ] {
        let stderr = serving(&arguments).stop();
        let lines: Vec<&str> = stderr
            .lines()
            .filter(|line| line.contains("--fixed-actor"))
            .collect();
        assert_eq!(
            lines.len(),
            2,
            "one line per fixed actor, with no request sent: {stderr}"
        );
        for (line, actor) in lines.iter().zip(order) {
            assert!(line.contains(actor), "{order:?}: {line}");
            assert!(line.contains("not authentication"), "{line}");
        }
        let (first, second) = (lines[0], lines[1]);
        let attributes = |actor: &str| if actor == CLERK { "badge" } else { "key" };
        assert!(first.contains(attributes(order[0])), "{first}");
        assert!(second.contains(attributes(order[1])), "{second}");
    }
}

#[test]
fn two_fixed_actors_granted_one_command_take_the_first_given() {
    let (clerk, keeper) = (clerk(), keeper());
    for (arguments, shift) in [
        (clerk_then_keeper(&clerk, &keeper), "early"),
        (keeper_then_clerk(&clerk, &keeper), "late"),
    ] {
        let served = serving(&arguments);
        let (status, opened) = open(&served, DESK, "window");
        assert_eq!(status, 202, "{opened}");
        let (status, noted) = call(&served, NOTE, Some(&json!({"desk_id": DESK})));
        assert_eq!(status, 202, "{noted}");
        assert_eq!(
            noted["published"][0]["payload"]["shift"], shift,
            "NoteDesk, granted to both, is sent as the first given: {noted}"
        );
    }
}

#[test]
fn a_read_granted_view_is_read_as_the_first_fixed_actor_that_may_read_it() {
    let (clerk, keeper) = (clerk(), keeper());
    let served = serving(&clerk_then_keeper(&clerk, &keeper));
    let (status, rows) = call(&served, LEDGER, None);
    assert_eq!(status, 200, "the keeper, given second, may read it: {rows}");
    drop(served);
    let served = serving(&["--fixed-actor", CLERK, "--fixed-caller", &clerk]);
    let (status, body) = call(&served, LEDGER, None);
    assert_eq!(status, 403, "{body}");
    assert_eq!(body, json!({"refused": "not granted", "actor": CLERK}));
}

#[test]
fn each_fixed_actor_needs_its_own_fixed_caller() {
    let (clerk, keeper) = (clerk(), keeper());
    for arguments in [
        vec![
            "--fixed-actor",
            CLERK,
            "--fixed-actor",
            KEEPER,
            "--fixed-caller",
            &keeper,
        ],
        vec![
            "--fixed-actor",
            CLERK,
            "--fixed-caller",
            &clerk,
            "--fixed-actor",
            CLERK,
            "--fixed-caller",
            &clerk,
        ],
    ] {
        let Err((code, stdout, stderr)) = serve(&arguments) else {
            panic!("{arguments:?} listened");
        };
        assert_eq!(code, Some(1), "{arguments:?}: {stderr}");
        assert!(
            !stdout.contains("listening"),
            "{arguments:?} listened: {stdout}"
        );
        assert!(stderr.contains("--fixed-"), "{arguments:?}: {stderr}");
    }
}
