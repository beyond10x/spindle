//! Adversary cases of wave 26, unit U3 (`story:run-fails-on-rejected-line`,
//! `story:clock-and-mint-in-every-mode`): the fraction of an RFC 3339 instant given to `--clock` or
//! `"at"`, `"at"` spelled with an offset, a malformed `"at"`, a seeded mint over state a run kept,
//! the `identity.mint = seeded` binding on a served run given `--mint-seed`, and the answer a
//! served run handed back just before a seed that rejected an entry stops it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const CLOCK: &str = "crates/spindle-cli/tests/fixtures/clock";
const MINT: &str = "crates/spindle-kernel/tests/fixtures/mint";
const SERVED_WORLD: &str = "crates/spindle-cli/tests/fixtures/served-world";
const SEED_SPEC: &str = "crates/spindle-cli/tests/fixtures/seed";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BOOK: &str = "ledger.slot.Book";
const ADD_BOOK: &str = "shelf.books.AddBook";
const GRANT: &str = "vault.access.GrantRole";
const VOUCH: &str = "vault.access.VouchFor";
const OPERATOR: &str = "vault.access.Operator";
const VISITOR: &str = "vault.access.Visitor";
const VOUCHED: &str = "external.vault.access.VouchFor.vouched";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one exchange, or a run's exit, may take.
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
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    if let Err(error) = written {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "the lines are written: {error}"
        );
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

/// One stdin line booking `booking` with `deadline`, at `at` where given.
fn book(booking: &str, deadline: &str, at: Option<&str>) -> String {
    let mut line = json!({"command": BOOK, "input": {"booking_id": booking, "deadline": deadline}});
    if let Some(at) = at {
        line["at"] = json!(at);
    }
    format!("{line}\n")
}

fn clocked(clock: &str, lines: &str) -> Output {
    run(
        &["run", CLOCK, "--environment", MEMORY, "--clock", clock],
        lines,
    )
}

// --- the fraction of an instant ------------------------------------------------------------------

/// `--clock 2026-01-01T00:00:00.5Z` stands half a second past midnight, so a deadline three tenths
/// of a second past midnight has passed: `deadline > now` is false and the slot is booked late.
#[test]
fn a_clock_with_a_fraction_stands_at_that_fraction() {
    let output = clocked(
        "2026-01-01T00:00:00.5Z",
        &book("b1", "2026-01-01T00:00:00.3Z", None),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        results(&output)[0]["outcome"],
        "late",
        "a clock at 00:00:00.5 is past a deadline at 00:00:00.3: {output:?}"
    );
}

/// An `"at"` a quarter of a second past midnight is before a clock standing at half a second past
/// it: the line is rejected and the run exits 1.
#[test]
fn an_at_before_the_clock_by_a_fraction_is_rejected() {
    let output = clocked(
        "2026-01-01T00:00:00.5Z",
        &book(
            "b1",
            "2026-06-01T00:00:00Z",
            Some("2026-01-01T00:00:00.25Z"),
        ),
    );
    let printed = results(&output);
    assert!(
        printed[0]["failure"].is_object(),
        "00:00:00.25 is before the clock at 00:00:00.5: {printed:?}"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

// --- "at" spelled with an offset, equal to the clock, malformed, without --clock ------------------

/// `"at"` equal to the clock's instant, spelled with an offset, runs; one an offset puts after the
/// clock in UTC although its local reading is earlier runs too; an instant before the clock
/// rejects only its line, named on stderr, and the run exits 1 after answering every line.
#[test]
fn an_at_is_read_as_an_instant_whatever_its_offset() {
    let lines = format!(
        "{}{}{}{}",
        book(
            "b1",
            "2026-06-01T00:00:00Z",
            Some("2026-02-01T01:00:00+01:00")
        ),
        book(
            "b2",
            "2026-02-01T00:30:00Z",
            Some("2026-01-31T19:30:00-05:00")
        ),
        book("b3", "2026-06-01T00:00:00Z", Some("2026-02-01T00:00:00Z")),
        book("b4", "2026-06-01T00:00:00Z", None),
    );
    let output = clocked("2026-02-01T00:00:00Z", &lines);
    let printed = results(&output);
    assert_eq!(printed.len(), 4, "every line is answered: {output:?}");
    assert_eq!(printed[0]["outcome"], "early", "{printed:?}");
    assert_eq!(
        printed[1]["outcome"], "late",
        "19:30 at -05:00 is 00:30 UTC, the deadline itself: {printed:?}"
    );
    assert!(
        printed[2]["failure"].is_object(),
        "00:00 UTC is before the clock at 00:30 UTC: {printed:?}"
    );
    assert_eq!(printed[3]["outcome"], "early", "{printed:?}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let named: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains("line "))
        .collect();
    assert!(named.iter().any(|line| line.contains("line 3")), "{stderr}");
    assert!(
        named.iter().all(|line| !line.contains("line 1")
            && !line.contains("line 2")
            && !line.contains("line 4")),
        "{stderr}"
    );
}

/// A malformed `"at"` and an `"at"` on a run without `--clock` each reject their line; the lines
/// around them run.
#[test]
fn a_malformed_at_and_an_at_without_a_clock_are_rejected() {
    let lines = format!(
        "{}{}{}",
        book("b1", "2026-06-01T00:00:00Z", Some("2026-02-30T00:00:00Z")),
        book("b2", "2026-06-01T00:00:00Z", Some("2026-02-01 00:00:00Z")),
        book("b3", "2026-06-01T00:00:00Z", None),
    );
    let output = clocked("2026-02-01T00:00:00Z", &lines);
    let printed = results(&output);
    assert_eq!(printed.len(), 3, "{output:?}");
    assert!(printed[0]["failure"].is_object(), "{printed:?}");
    assert!(printed[1]["failure"].is_object(), "{printed:?}");
    assert_eq!(printed[2]["outcome"], "early", "{printed:?}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");

    let unclocked = run(
        &["run", CLOCK, "--environment", MEMORY],
        &book("b1", "2026-06-01T00:00:00Z", Some("2030-01-01T00:00:00Z")),
    );
    assert!(
        results(&unclocked)[0]["failure"].is_object(),
        "{unclocked:?}"
    );
    assert_eq!(unclocked.status.code(), Some(1), "{unclocked:?}");
}

// --- a seeded mint over state the run kept ---------------------------------------------------------

/// The ticket identity a stdin run with `--mint-seed 7` on `state` minted for one `OpenTicket`.
fn ticket(state: &Path, title: &str) -> String {
    let line = format!(
        "{}\n",
        json!({"command": "mint.ticket.OpenTicket", "input": {"title": title}})
    );
    let output = run(
        &[
            "run",
            MINT,
            "--environment",
            LAPTOP,
            "--state",
            state.to_str().expect("a UTF-8 path"),
            "--mint-seed",
            "7",
        ],
        &line,
    );
    let printed = results(&output);
    assert_eq!(
        printed.first().map(|result| &result["outcome"]),
        Some(&json!("opened")),
        "the ticket {title} opens: {output:?}"
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    printed[0]["events"][0]["payload"]["ticket_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a minted identity: {output:?}"))
        .to_owned()
}

/// Two stdin runs on one kept state with one `--mint-seed`: the second run's first draw is the
/// identity the first run's ticket holds, and a seeded mint never mints an identity an instance
/// already holds (`Seeded`, as `reset_to` avoids a world's rows).
#[test]
fn a_seeded_stdin_run_on_kept_state_mints_no_identity_the_state_holds() {
    let directory = scratch();
    let state = directory.path().join("state.sqlite");
    let first = ticket(&state, "first");
    let second = ticket(&state, "second");
    assert_ne!(
        second, first,
        "the second run minted the identity the first run's ticket holds"
    );
}

// --- served runs -------------------------------------------------------------------------------

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
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

impl Served {
    /// The exit code once the run exits by itself, within [`ANSWER`].
    fn exited(&mut self) -> Option<i32> {
        let deadline = Instant::now() + ANSWER;
        loop {
            if let Some(status) = self.child.try_wait().expect("the run is polled") {
                return status.code();
            }
            assert!(
                Instant::now() < deadline,
                "the run did not exit within {ANSWER:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// `run` on `spec` with `arguments`: the running process, or the exit code and stderr of one that
/// stopped without listening.
fn start(spec: &str, arguments: &[&str]) -> Result<Served, (Option<i32>, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("run")
        .arg(spec)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        return Err((status.code(), stderr));
    }
    // Stderr drained beside the run, so a long log never blocks it.
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { return };
                eprintln!("{line}");
            }
        });
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served { child, address })
}

fn serve(spec: &str, arguments: &[&str]) -> Served {
    start(spec, arguments).unwrap_or_else(|stopped| panic!("run did not listen: {stopped:?}"))
}

/// The method and path ESS publishes for the operation `id` of `spec`'s component `component`.
fn route(spec: &str, component: &str, id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
    let declared = loaded
        .ir
        .components()
        .values()
        .find(|declared| declared.name.as_str() == component)
        .expect("the fixture declares the component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, declared))
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

/// One HTTP/1.1 exchange on its own connection, `body` sent as JSON: every byte answered, empty
/// where the connection closed without an answer.
fn raw(address: SocketAddr, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    String::from_utf8_lossy(&answer).into_owned()
}

/// The status and JSON body of one exchange.
fn sent(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    let answer = raw(address, method, path, body);
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

/// Adds a book titled `title` on the served-world run at `address`: the identity it minted.
fn add(address: SocketAddr, title: &str) -> String {
    let (method, path) = route(SERVED_WORLD, "library", ADD_BOOK);
    let (status, answer) = sent(
        address,
        &method,
        &path,
        &json!({"title": title}).to_string(),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["published"][0]["payload"]["book_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the added book's identity is published: {answer}"))
        .to_owned()
}

/// Two starts of a served run on one kept state with one `--mint-seed`: the second start's first
/// draw is the identity the first start's book holds, and a seeded mint never mints an identity
/// an instance already holds.
#[test]
fn a_seeded_served_run_on_kept_state_mints_no_identity_the_state_holds() {
    let directory = scratch();
    let state = directory.path().join("state");
    let state = state.to_str().expect("a UTF-8 path");
    let arguments = [
        "--environment",
        LAPTOP,
        "--answer-default",
        "--state",
        state,
        "--mint-seed",
        "7",
    ];
    let first = {
        let served = serve(SERVED_WORLD, &arguments);
        add(served.address, "One")
    };
    let served = serve(SERVED_WORLD, &arguments);
    let second = add(served.address, "Two");
    assert_ne!(
        second, first,
        "the second start minted the identity the first start's book holds"
    );
}

/// A served run on an environment that binds `identity.mint` to `seeded`, given `--mint-seed`,
/// listens and mints from that seed: the refusal names `--mint-seed` as the way to serve it.
#[test]
fn a_served_seeded_mint_binding_given_a_mint_seed_listens() {
    let directory = scratch();
    let environment = directory.path().join("seeded.toml");
    std::fs::write(
        &environment,
        "format = \"spindle-environment/1\"\nname = \"seeded\"\nos = \"linux\"\narch = \"x86_64\"\n\
         \n[provides]\n\"store.durable\" = [\"memory\"]\n\"identity.mint\" = [\"seeded\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n",
    )
    .expect("the environment is written");
    let environment = environment.to_str().expect("a UTF-8 path");
    let seeded = match start(
        SERVED_WORLD,
        &[
            "--environment",
            environment,
            "--answer-default",
            "--mint-seed",
            "7",
        ],
    ) {
        Ok(served) => add(served.address, "One"),
        Err((code, stderr)) => panic!(
            "a seeded binding given --mint-seed 7 is refused ({code:?}), the refusal asking for \
             --mint-seed: {stderr}"
        ),
    };
    let laptop = add(
        serve(
            SERVED_WORLD,
            &[
                "--environment",
                LAPTOP,
                "--answer-default",
                "--mint-seed",
                "7",
            ],
        )
        .address,
        "One",
    );
    assert_eq!(seeded, laptop, "both mint from seed 7");
}

// --- the answers in hand when a seed that rejected an entry stops the run -----------------------

/// A desk on 127.0.0.1 answering one vouch request `{"holds": true}`, but only once `release`
/// is sent; `asked` is sent once the request has arrived.
fn held_desk() -> (SocketAddr, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
    let address = listener.local_addr().expect("the desk has an address");
    let (asked, arrived) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(ANSWER));
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).is_err() {
                return;
            }
            let header = header.trim_end().to_ascii_lowercase();
            if header.is_empty() {
                break;
            }
            if let Some(value) = header.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let _ = asked.send(());
        let _ = released.recv_timeout(STARTUP);
        let body = json!({"holds": true}).to_string();
        let _ = write!(
            reader.into_inner(),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (address, arrived, release)
}

/// The seed's first entry is held at a desk while two grants arrive over HTTP; once released, the
/// loop takes one grant between the two entries, then sends the last entry, which is rejected, and
/// the seed ends there and stops the run. The grant the loop took is an answer in hand and reads
/// 2xx; the grant still queued at the stop is not taken and reads 503, never a closed connection
/// (`story:serve-loop-signal-exit-answers`, which removed the `ignore` this case carried).
#[test]
fn the_answers_in_hand_are_written_before_a_rejecting_seed_stops_the_run() {
    let directory = scratch();
    let (desk, asked, release) = held_desk();
    let seed = directory.path().join("seed.json");
    std::fs::write(
        &seed,
        json!([
            {"command": VOUCH, "input": {"voucher_id": "v-1", "role_id": "admin"},
             "actor": VISITOR, "request_id": "seed-vouch"},
            {"command": GRANT, "input": {"role_id": "spy", "title": "Spy"}, "actor": VISITOR,
             "request_id": "seed-visitor-grant"},
        ])
        .to_string(),
    )
    .expect("the seed is written");
    let environment = directory.path().join("environment.toml");
    std::fs::write(
        &environment,
        format!(
            "format = \"spindle-environment/1\"\nname = \"seeded\"\nos = \"linux\"\narch = \"x86_64\"\n\
             \n[provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n\
             \"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\n\
             channel = [\"in-process\"]\nprojection = [\"in-process\"]\n\
             \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n\
             \"{VOUCHED}\" = [\"desk\"]\n\n[http.desk]\nmethod = \"POST\"\n\
             url = \"http://{desk}/vouch\"\ntimeout_ms = 30000\n"
        ),
    )
    .expect("the environment is written");
    let caller = json!({"tenant": "north"}).to_string();
    let mut served = serve(
        SEED_SPEC,
        &[
            "--environment",
            environment.to_str().expect("a UTF-8 path"),
            "--seed",
            seed.to_str().expect("a UTF-8 path"),
            "--fixed-actor",
            OPERATOR,
            "--fixed-caller",
            &caller,
        ],
    );
    asked
        .recv_timeout(ANSWER)
        .expect("the seed's first entry asks the desk");
    let (method, path) = route(SEED_SPEC, "gate", GRANT);
    let address = served.address;
    let grants: Vec<_> = ["r1", "r2"]
        .into_iter()
        .map(|role| {
            let (method, path) = (method.clone(), path.clone());
            let body = json!({"role_id": role, "title": role}).to_string();
            std::thread::spawn(move || raw(address, &method, &path, &body))
        })
        .collect();
    // Both grants reach the loop's queue while the seed is held at its first entry.
    std::thread::sleep(Duration::from_millis(500));
    release.send(()).expect("the desk is released");
    let answers: Vec<String> = grants
        .into_iter()
        .map(|grant| grant.join().expect("the grant's thread ends"))
        .collect();
    assert_eq!(served.exited(), Some(1), "the rejecting seed stops the run");
    let taken = answers
        .iter()
        .filter(|answer| answer.starts_with("HTTP/1.1 2"))
        .count();
    let queued = answers
        .iter()
        .filter(|answer| answer.starts_with("HTTP/1.1 503"))
        .count();
    assert_eq!(
        (taken, queued),
        (1, 1),
        "the grant the loop took reads 2xx, the one still queued at the stop 503: {answers:#?}"
    );
}
