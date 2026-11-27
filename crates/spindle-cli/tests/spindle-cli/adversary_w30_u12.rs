//! Adversary of wave 30 U12 (story:served-clock-steps): a served run with `--clock` reads its
//! clock at `GET /api/clock`, steps it forward with `POST /api/clock` and a reset world's
//! `"clock"`, and every record of `GET /api/events` carries the instant its command ran.
//!
//! Six cases. The clock holds its instant to the millisecond, as `GET /api/clock` and a step's
//! answer spell it, so after a step to a sub-millisecond instant a step to the instant the run
//! answered is a step to where the clock stands and is answered `200`. A clock before 1970 is
//! refused wherever an instant sets it: `--clock` exits 2, and `POST /api/clock` and a reset
//! world's `"clock"` answer `400` and move nothing.
//!
//! The other four are probes: a step to where the clock stands and a reset world at that instant
//! move nothing and are answered `200`; a malformed step, a leap second included, is refused `400`
//! and moves nothing; an offset and a fraction of a second are read; and a reset world whose rows
//! are refused leaves the clock where it stood.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const CLOCK_SERVED: &str = "crates/spindle-cli/tests/fixtures/clock-served";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BOOK: &str = "ledger.slot.Book";
const BOOKING: &str = "ledger.slot.Booking";
const DEADLINE: &str = "2026-06-01T00:00:00Z";
const FEBRUARY: &str = "2026-02-01T00:00:00Z";
const JULY: &str = "2026-07-01T00:00:00Z";
const CLOCK_PATH: &str = "/api/clock";

/// How long `run` may take to print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on `clock-served` and the laptop environment with `--clock clock`, its state in a
/// directory of its own.
fn serve(clock: &str) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            CLOCK_SERVED,
            "--environment",
            LAPTOP,
            "--answer-default",
            "--clock",
            clock,
        ])
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
        _state: state,
    }
}

/// One HTTP/1.1 exchange on its own connection, `body` sent as is: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
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

/// The path ESS publishes for `ledger.slot.Book`, a `POST`.
fn book_path() -> String {
    let loaded = spindle_spec::load(&root().join(CLOCK_SERVED)).expect("the fixture compiles");
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
            if operation["operationId"] == BOOK {
                assert_eq!(method, "post", "{BOOK} is a POST");
                return path.clone();
            }
        }
    }
    panic!("the contract declares no operation {BOOK}")
}

/// Books `booking` with the deadline `deadline`: the outcome.
fn booked(served: &Served, booking: &str) -> Value {
    let body = json!({"booking_id": booking, "deadline": DEADLINE}).to_string();
    let (status, answer) = exchange(served.address, "POST", &book_path(), &body);
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["outcome"].clone()
}

/// `POST /api/clock` with `body` as is.
fn step_raw(served: &Served, body: &str) -> (u16, Value) {
    exchange(served.address, "POST", CLOCK_PATH, body)
}

/// `POST /api/clock` to `at`.
fn step(served: &Served, at: &str) -> (u16, Value) {
    step_raw(served, &json!({"at": at}).to_string())
}

/// `GET /api/clock`.
fn clock(served: &Served) -> (u16, Value) {
    exchange(served.address, "GET", CLOCK_PATH, "")
}

/// `POST /api/reset` with `world`.
fn reset(served: &Served, world: &Value) -> (u16, Value) {
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

/// A step to a sub-millisecond instant sets the clock to that instant's millisecond, as
/// `GET /api/clock` answers it; a step to the instant the run just answered is then a step to where
/// the clock stands and is answered `200`.
#[test]
fn a_step_to_the_instant_get_answered_moves_nothing_and_is_answered() {
    let served = serve(FEBRUARY);
    let (status, stepped) = step(&served, "2026-07-01T00:00:00.0005Z");
    let (read_status, read) = clock(&served);
    assert_eq!(read_status, 200, "{read}");
    if status == 200 {
        assert_eq!(
            stepped, read,
            "the step answers where the clock stands, as GET does"
        );
    }
    let at = read["at"]
        .as_str()
        .unwrap_or_else(|| panic!("an instant: {read}"))
        .to_owned();
    assert_eq!(
        step(&served, &at),
        (200, json!({"at": at})),
        "a step to the instant GET /api/clock answered is a step to where the clock stands"
    );
}

/// A clock before 1970 is refused wherever an instant sets it (correction round 3, F2): `--clock`
/// exits 2 naming the flag and the instant, and `POST /api/clock` and a reset world's `"clock"`
/// answer `400` and move nothing; the run's rows are left as they were.
#[test]
fn a_clock_before_1970_is_refused() {
    const LANDING: &str = "1969-07-20T20:17:40Z";
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let refused = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            CLOCK_SERVED,
            "--environment",
            LAPTOP,
            "--answer-default",
            "--clock",
            LANDING,
        ])
        .arg("--state")
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("--clock") && stderr.contains(LANDING) && stderr.contains("1970"),
        "the refusal names the flag, the instant and the limit: {stderr}"
    );

    let served = serve(FEBRUARY);
    assert_eq!(booked(&served, "b1"), "early");
    let (status, answer) = step(&served, LANDING);
    assert_eq!(status, 400, "a step before 1970: {answer}");
    let (status, answer) = reset(&served, &json!({"clock": LANDING}));
    assert_eq!(status, 400, "a world before 1970: {answer}");
    assert_eq!(clock(&served), (200, json!({"at": FEBRUARY})));
    assert_eq!(
        events(&served).len(),
        1,
        "the refused world emptied nothing"
    );
}

/// Green probe. A step to the instant the clock stands at, and a reset world at it, move nothing
/// and are answered `200`; a later command still reads that instant.
#[test]
fn a_step_and_a_world_at_the_clock_instant_are_answered() {
    let served = serve(FEBRUARY);
    assert_eq!(step(&served, FEBRUARY), (200, json!({"at": FEBRUARY})));
    let (status, answer) = reset(&served, &json!({"clock": FEBRUARY}));
    assert_eq!(status, 200, "a world at the clock's instant: {answer}");
    assert_eq!(clock(&served), (200, json!({"at": FEBRUARY})));
    assert_eq!(booked(&served, "b1"), "early");
    assert_eq!(events(&served)[0]["at"], FEBRUARY);
}

/// Green probe. A step whose body is no `{"at": "<RFC 3339>"}` is refused `400` and moves nothing:
/// a leap second, a date, a number, an extra key, no body, no JSON.
#[test]
fn a_malformed_step_is_refused_400_and_moves_nothing() {
    let served = serve(FEBRUARY);
    for body in [
        r#"{"at": "2026-06-30T23:59:60Z"}"#,
        r#"{"at": "2026-07-01"}"#,
        r#"{"at": "2026-07-01T00:00:00"}"#,
        r#"{"at": "2026-07-01T00:00:00.0000000001Z"}"#,
        r#"{"at": 1782864000}"#,
        r#"{"at": "2026-07-01T00:00:00Z", "by": "a consumer"}"#,
        r#"{"when": "2026-07-01T00:00:00Z"}"#,
        r#"["2026-07-01T00:00:00Z"]"#,
        "2026-07-01T00:00:00Z",
        "",
    ] {
        let (status, answer) = step_raw(&served, body);
        assert_eq!(status, 400, "{body:?}: {answer}");
        assert!(answer["refused"].is_string(), "{body:?}: {answer}");
    }
    assert_eq!(clock(&served), (200, json!({"at": FEBRUARY})));
}

/// Green probe. A step reads an offset and a fraction of a second, and answers the instant in UTC
/// as ESS spells it; a command after it reads the instant.
#[test]
fn a_step_reads_an_offset_and_a_fraction() {
    let served = serve(FEBRUARY);
    assert_eq!(
        step(&served, "2026-07-01T02:00:00+02:00"),
        (200, json!({"at": JULY}))
    );
    assert_eq!(
        step(&served, "2026-07-01T00:00:00.250Z"),
        (200, json!({"at": "2026-07-01T00:00:00.25Z"}))
    );
    assert_eq!(
        step(&served, "2026-06-30T20:00:01-04:00"),
        (200, json!({"at": "2026-07-01T00:00:01Z"}))
    );
    assert_eq!(booked(&served, "b1"), "late");
    assert_eq!(events(&served)[0]["at"], "2026-07-01T00:00:01Z");
}

/// Green probe. A reset world whose clock is later but whose rows are refused (two rows of one
/// instance) is refused `400`, empties nothing and leaves the clock where it stood.
#[test]
fn a_refused_world_leaves_the_clock() {
    let served = serve(FEBRUARY);
    assert_eq!(booked(&served, "b1"), "early");
    let row = json!({"entity": BOOKING, "identity": "b9", "state": "Booked",
                     "fields": {"deadline": DEADLINE}});
    let (status, answer) = reset(&served, &json!({"clock": JULY, "rows": [row, row]}));
    assert_eq!(status, 400, "two rows of one instance: {answer}");
    assert_eq!(clock(&served), (200, json!({"at": FEBRUARY})));
    let listed = events(&served);
    assert_eq!(
        listed.len(),
        1,
        "the refused reset emptied nothing: {listed:?}"
    );
    assert_eq!(booked(&served, "b2"), "early");

    let (status, answer) = reset(&served, &json!({"clock": JULY, "rows": [row]}));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(clock(&served), (200, json!({"at": JULY})));
    assert_eq!(booked(&served, "b3"), "late");
}
