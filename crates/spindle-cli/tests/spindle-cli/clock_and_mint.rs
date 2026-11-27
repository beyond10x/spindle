//! `run --clock <RFC 3339>` and `run --mint-seed <n>` (`story:clock-and-mint-in-every-mode`): a run
//! reading stdin or serving HTTP starts on a manual clock at that instant and mints identities from
//! that seed; on stdin a line's `"at"` moves the clock forward before its command runs, and one
//! earlier than the clock is rejected.
//!
//! The fixture `tests/fixtures/clock` (in process, read from stdin) and `tests/fixtures/clock-served`
//! (the same domain, reached over the network) book a slot `early` when its deadline is after
//! `now` (`deadline > now`) and `late` otherwise. Every case books with the deadline
//! 2026-06-01T00:00:00Z: early on February 1st 2026, late on the system clock of any day the test
//! runs on. The mint cases run `tests/fixtures/mint` of the kernel on stdin and
//! `tests/fixtures/served-world` over HTTP.
//!
//! A served run with `--clock` steps its clock (`story:served-clock-steps`): `POST /api/clock`
//! `{"at"}` and a reset world's `"clock"` move it forward between commands, `GET /api/clock`
//! answers where it stands, and every record of `GET /api/events` carries its command's `at`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const CLOCK: &str = "crates/spindle-cli/tests/fixtures/clock";
const CLOCK_SERVED: &str = "crates/spindle-cli/tests/fixtures/clock-served";
const MINT: &str = "crates/spindle-kernel/tests/fixtures/mint";
const SERVED_WORLD: &str = "crates/spindle-cli/tests/fixtures/served-world";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BOOK: &str = "ledger.slot.Book";
const ADD_BOOK: &str = "shelf.books.AddBook";
const DEADLINE: &str = "2026-06-01T00:00:00Z";

/// How long `run` may take to print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// `run` reading `lines` on stdin.
fn run(arguments: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the lines are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

/// One stdin line booking `booking`, at `at` where given.
fn book(booking: &str, at: Option<&str>) -> String {
    let mut line = json!({"command": BOOK, "input": {"booking_id": booking, "deadline": DEADLINE}});
    if let Some(at) = at {
        line["at"] = json!(at);
    }
    format!("{line}\n")
}

/// Three tickets and a label opened on the kernel's mint fixture, every identity minted.
const OPENED: &str = concat!(
    r#"{"command": "mint.ticket.OpenTicket", "input": {"title": "first"}}"#,
    "\n",
    r#"{"command": "mint.ticket.OpenTicket", "input": {"title": "second"}}"#,
    "\n",
    r#"{"command": "mint.label.OpenLabel", "input": {"text": "fragile"}}"#,
    "\n",
);

#[test]
fn a_seeded_stdin_run_mints_the_same_identities_twice() {
    let minted = |seed: &str| {
        let output = run(
            &["run", MINT, "--environment", LAPTOP, "--mint-seed", seed],
            OPENED,
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let printed = results(&output);
        assert_eq!(printed.len(), 3, "{printed:?}");
        assert!(
            printed.iter().all(|result| result["outcome"] == "opened"),
            "{printed:?}"
        );
        output.stdout
    };
    let once = minted("7");
    assert_eq!(
        minted("7"),
        once,
        "one seed prints byte-identical results, minted identities included"
    );
    assert_ne!(minted("8"), once, "another seed mints other identities");
}

#[test]
fn a_line_moves_the_clock() {
    let clocked = run(
        &[
            "run",
            CLOCK,
            "--environment",
            MEMORY,
            "--clock",
            "2026-01-01T00:00:00Z",
        ],
        &book("b1", Some("2026-02-01T00:00:00Z")),
    );
    assert_eq!(clocked.status.code(), Some(0), "{clocked:?}");
    assert_eq!(results(&clocked)[0]["outcome"], "early", "{clocked:?}");

    let unclocked = run(&["run", CLOCK, "--environment", MEMORY], &book("b1", None));
    assert_eq!(unclocked.status.code(), Some(0), "{unclocked:?}");
    assert_eq!(
        results(&unclocked)[0]["outcome"],
        "late",
        "the system clock is past the deadline: {unclocked:?}"
    );
}

#[test]
fn a_line_with_an_earlier_at_is_rejected() {
    let lines = format!(
        "{}{}{}",
        book("b1", Some("2026-02-01T00:00:00Z")),
        book("b2", Some("2026-01-15T00:00:00Z")),
        book("b3", None),
    );
    let output = run(
        &[
            "run",
            CLOCK,
            "--environment",
            MEMORY,
            "--clock",
            "2026-01-01T00:00:00Z",
        ],
        &lines,
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = results(&output);
    assert_eq!(printed.len(), 3, "{printed:?}");
    assert_eq!(printed[0]["outcome"], "early");
    assert!(
        printed[1]["failure"].is_object(),
        "an instant before the clock rejects the line: {printed:?}"
    );
    assert_eq!(
        printed[2]["outcome"], "early",
        "the clock stays on February 1st: {printed:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("rejected") && line.contains("line 2")),
        "{stderr}"
    );
}

/// A line rejected before it ran (here, a command the specification does not declare) does not
/// move the clock to its `"at"`: the next line, without one, books on the clock as it stood, and an
/// `"at"` between the two instants runs.
#[test]
fn a_rejected_line_leaves_the_clock_where_it_was() {
    let rejected = json!({"command": "ledger.slot.Undeclared", "input": {},
                          "at": "2026-03-01T00:00:00Z"});
    let unmoved = json!({"command": BOOK,
                         "input": {"booking_id": "b1", "deadline": "2026-02-15T00:00:00Z"}});
    let lines = format!(
        "{rejected}\n{unmoved}\n{}",
        book("b2", Some("2026-02-10T00:00:00Z"))
    );
    let output = run(
        &[
            "run",
            CLOCK,
            "--environment",
            MEMORY,
            "--clock",
            "2026-02-01T00:00:00Z",
        ],
        &lines,
    );
    let printed = results(&output);
    assert_eq!(printed.len(), 3, "{output:?}");
    assert!(printed[0]["failure"].is_object(), "{printed:?}");
    assert_eq!(
        printed[1]["outcome"], "early",
        "the clock stands at February 1st, not at the rejected line's March 1st: {printed:?}"
    );
    assert_eq!(
        printed[2]["outcome"], "early",
        "February 10th is after the clock: {printed:?}"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn a_seeded_mint_binding_is_served() {
    let directory = scratch();
    let environment = directory.path().join("seeded.toml");
    std::fs::write(
        &environment,
        "format = \"spindle-environment/1\"\nname = \"seeded\"\nos = \"linux\"\narch = \"x86_64\"\n\
         \n[provides]\n\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"seeded\"]\n",
    )
    .expect("the environment is written");
    let environment = environment.to_str().expect("a UTF-8 path");
    let minted = |extra: &[&str]| {
        let mut arguments = vec!["run", MINT, "--environment", environment];
        arguments.extend(extra);
        let output = run(&arguments, OPENED);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            results(&output)
                .iter()
                .all(|result| result["outcome"] == "opened"),
            "{output:?}"
        );
        output.stdout
    };
    let once = minted(&[]);
    assert_eq!(minted(&[]), once, "two runs print byte-identical results");
    assert_eq!(
        minted(&["--mint-seed", "0"]),
        once,
        "without --mint-seed the seed is 0"
    );
    assert_ne!(minted(&["--mint-seed", "1"]), once, "the flag's seed mints");
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    spec: &'static str,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on `spec` and the laptop environment, its state in a directory of its own, with `extra`.
fn serve(spec: &'static str, extra: &[&str]) -> Served {
    let state = scratch();
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--answer-default"])
        .args(extra)
        .arg("--state")
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        spec,
        _state: state,
    }
}

/// The method and path ESS publishes for the operation `id` of `spec`'s one component.
fn route(spec: &str, id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .next()
        .expect("the fixture declares a component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

/// One HTTP/1.1 exchange on its own connection, `body` sent as is: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    exchange_with(address, method, path, "", body)
}

/// [`exchange`], `head` the header lines written after `Host`, each ending in `\r\n`.
fn exchange_with(
    address: SocketAddr,
    method: &str,
    path: &str,
    head: &str,
    body: &str,
) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{head}Connection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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

fn call(served: &Served, id: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(served.spec, id);
    exchange(served.address, &method, &path, &body.to_string())
}

/// `POST /api/reset` with `world` as its body, or none.
fn reset(served: &Served, world: Option<&Value>) {
    let body = world.map(Value::to_string).unwrap_or_default();
    let (status, answer) = exchange(served.address, "POST", "/api/reset", &body);
    assert_eq!(status, 200, "{answer}");
}

/// Adds a book titled `title` and answers the identity the run minted for it.
fn add(served: &Served, title: &str) -> String {
    let (status, answer) = call(served, ADD_BOOK, &json!({"title": title}));
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["published"][0]["payload"]["book_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the added book's identity is published: {answer}"))
        .to_owned()
}

#[test]
fn a_served_run_mints_from_the_seed() {
    let first = add(&serve(SERVED_WORLD, &["--mint-seed", "7"]), "One");
    let again = add(&serve(SERVED_WORLD, &["--mint-seed", "7"]), "One");
    assert_eq!(again, first, "one seed, the same first identity");
    let other = add(&serve(SERVED_WORLD, &["--mint-seed", "8"]), "One");
    assert_ne!(other, first, "another seed, another identity");
}

#[test]
fn a_served_run_starts_on_the_clock_flag() {
    let booked = |extra: &[&str]| {
        let served = serve(CLOCK_SERVED, extra);
        let (status, answer) = call(
            &served,
            BOOK,
            &json!({"booking_id": "b1", "deadline": DEADLINE}),
        );
        assert!((200..300).contains(&status), "{status} {answer}");
        answer["outcome"].clone()
    };
    assert_eq!(booked(&["--clock", "2026-02-01T00:00:00Z"]), "early");
    assert_eq!(booked(&[]), "late", "the system clock is past the deadline");
}

#[test]
fn a_reset_world_mint_overrides_the_seed_until_the_next_reset() {
    let served = serve(SERVED_WORLD, &["--mint-seed", "7"]);
    let seeded = add(&served, "One");
    reset(&served, Some(&json!({"mint": 9})));
    let world = add(&served, "One");
    assert_ne!(world, seeded, "the world's mint wins over the run's seed");
    let nine = add(&serve(SERVED_WORLD, &["--mint-seed", "9"]), "One");
    assert_eq!(world, nine, "the world's mint mints as seed 9 does");
}

#[test]
fn a_plain_reset_mints_from_the_run_seed_again() {
    let served = serve(SERVED_WORLD, &["--mint-seed", "7"]);
    let first = [add(&served, "One"), add(&served, "Two")];
    reset(&served, Some(&json!({"mint": 9})));
    add(&served, "One");
    reset(&served, None);
    assert_eq!(
        [add(&served, "One"), add(&served, "Two")],
        first,
        "a reset with no world mints the run's first identities again"
    );
    reset(&served, None);
    assert_eq!(
        [add(&served, "One"), add(&served, "Two")],
        first,
        "and so does every plain reset"
    );
}

const FEBRUARY: &str = "2026-02-01T00:00:00Z";
const MARCH: &str = "2026-03-01T00:00:00Z";
const JULY: &str = "2026-07-01T00:00:00Z";
const CLOCK_PATH: &str = "/api/clock";

/// Books `booking` on a served `clock-served` run: the outcome.
fn booked(served: &Served, booking: &str) -> Value {
    let (status, answer) = call(
        served,
        BOOK,
        &json!({"booking_id": booking, "deadline": DEADLINE}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["outcome"].clone()
}

/// `POST /api/clock` to `at`: the status and the answer.
fn step(served: &Served, at: &str) -> (u16, Value) {
    exchange(
        served.address,
        "POST",
        CLOCK_PATH,
        &json!({"at": at}).to_string(),
    )
}

/// `GET /api/clock`: the status and the answer.
fn clock(served: &Served) -> (u16, Value) {
    exchange(served.address, "GET", CLOCK_PATH, "")
}

/// `POST /api/reset` with `world`: the status and the answer.
fn reset_answer(served: &Served, world: &Value) -> (u16, Value) {
    exchange(served.address, "POST", "/api/reset", &world.to_string())
}

/// The events `GET /api/events` answers.
fn events(served: &Served) -> Vec<Value> {
    let (status, answer) = exchange(served.address, "GET", "/api/events", "");
    assert_eq!(status, 200, "{answer}");
    answer["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events: {answer}"))
        .clone()
}

#[test]
fn a_served_clock_moves_forward_and_a_guard_reads_it() {
    let served = serve(CLOCK_SERVED, &["--clock", FEBRUARY]);
    assert_eq!(booked(&served, "b1"), "early");
    assert_eq!(step(&served, JULY), (200, json!({"at": JULY})));
    assert_eq!(
        booked(&served, "b2"),
        "late",
        "the guard reads the stepped clock, past the deadline"
    );
}

#[test]
fn a_served_clock_refuses_to_move_back() {
    let served = serve(CLOCK_SERVED, &["--clock", FEBRUARY]);
    assert_eq!(step(&served, JULY), (200, json!({"at": JULY})));
    let (status, answer) = step(&served, MARCH);
    assert_eq!(status, 409, "{answer}");
    assert!(answer["refused"].is_string(), "{answer}");
    assert_eq!(
        clock(&served),
        (200, json!({"at": JULY})),
        "the clock stands where it stood"
    );
    assert_eq!(booked(&served, "b1"), "late");
}

#[test]
fn a_served_run_without_clock_refuses_to_step() {
    let served = serve(CLOCK_SERVED, &[]);
    assert_eq!(booked(&served, "b1"), "late");
    let (status, answer) = step(&served, JULY);
    assert_eq!(status, 409, "POST {CLOCK_PATH}: {answer}");
    assert!(answer["refused"].is_string(), "{answer}");
    let (status, answer) = clock(&served);
    assert_eq!(status, 409, "GET {CLOCK_PATH}: {answer}");
    assert!(answer["refused"].is_string(), "{answer}");
    let (status, answer) = reset_answer(&served, &json!({"clock": JULY}));
    assert_eq!(status, 409, "a reset world carrying clock: {answer}");
    let events = events(&served);
    assert_eq!(events.len(), 1, "the reset emptied nothing: {events:?}");
    assert_eq!(events[0]["payload"]["booking_id"], "b1", "{events:?}");
}

#[test]
fn a_reset_world_moves_the_clock() {
    let served = serve(CLOCK_SERVED, &["--clock", FEBRUARY]);
    assert_eq!(booked(&served, "b1"), "early");
    let (status, answer) = reset_answer(&served, &json!({"clock": JULY}));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(clock(&served), (200, json!({"at": JULY})));
    assert!(events(&served).is_empty(), "the reset emptied the stores");
    assert_eq!(booked(&served, "b2"), "late");

    let (status, answer) = reset_answer(&served, &json!({"clock": MARCH}));
    assert_eq!(status, 400, "an earlier clock refuses the reset: {answer}");
    assert!(answer["refused"].is_string(), "{answer}");
    let events = events(&served);
    assert_eq!(
        events.len(),
        1,
        "the refused reset emptied nothing: {events:?}"
    );
    assert_eq!(events[0]["payload"]["booking_id"], "b2", "{events:?}");
    assert_eq!(clock(&served), (200, json!({"at": JULY})));
}

#[test]
fn every_event_record_carries_at() {
    let served = serve(CLOCK_SERVED, &["--clock", FEBRUARY]);
    assert_eq!(booked(&served, "b1"), "early");
    assert_eq!(step(&served, JULY), (200, json!({"at": JULY})));
    assert_eq!(booked(&served, "b2"), "late");

    let unpaged = events(&served);
    assert_eq!(unpaged.len(), 2, "{unpaged:?}");
    assert_eq!(unpaged[0]["at"], FEBRUARY, "{unpaged:?}");
    assert_eq!(unpaged[1]["at"], JULY, "{unpaged:?}");

    let (status, first) = exchange(served.address, "GET", "/api/events?limit=1", "");
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["events"][0]["at"], FEBRUARY, "{first}");
    let next = first["next"]
        .as_str()
        .unwrap_or_else(|| panic!("a cursor: {first}"));
    let (status, page) = exchange(
        served.address,
        "GET",
        &format!("/api/events?after={next}"),
        "",
    );
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["events"].as_array().map(Vec::len), Some(1), "{page}");
    assert_eq!(page["events"][0]["at"], JULY, "{page}");
    assert_eq!(page["events"][0]["payload"]["booking_id"], "b2", "{page}");
}

#[test]
fn the_clock_route_needs_the_proxy_secret() {
    const SECRET: &str = "served-clock-proxy-token-5e2a";
    let directory = scratch();
    let path = directory.path().join("proxy-secret");
    std::fs::write(&path, format!("{SECRET}\n")).expect("the secret file is written");
    let path = path.to_str().expect("a UTF-8 path");
    let served = serve(
        CLOCK_SERVED,
        &[
            "--clock",
            FEBRUARY,
            "--trusted-caller-header",
            "X-Spindle-Caller",
            "--trusted-caller-secret",
            path,
        ],
    );
    let body = json!({"at": JULY}).to_string();
    let (status, answer) = exchange(served.address, "POST", CLOCK_PATH, &body);
    assert_eq!(status, 403, "POST without the secret: {answer}");
    let (status, answer) = exchange(served.address, "GET", CLOCK_PATH, "");
    assert_eq!(status, 403, "GET without the secret: {answer}");
    let secret = format!("Spindle-Proxy-Secret: {SECRET}\r\n");
    assert_eq!(
        exchange_with(served.address, "GET", CLOCK_PATH, &secret, ""),
        (200, json!({"at": FEBRUARY})),
        "the refused step moved nothing"
    );
    assert_eq!(
        exchange_with(served.address, "POST", CLOCK_PATH, &secret, &body),
        (200, json!({"at": JULY})),
        "with the secret the route steps"
    );
}
