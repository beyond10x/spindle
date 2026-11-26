//! Adversary cases of wave 27, unit U1 (story:stdin-run-fires-bindings,
//! story:stdin-arrange-and-line-references): a stdin run that fires bindings, arranges rows and
//! resolves references to earlier lines, attacked where the unit's own tests are silent.
//!
//! Fixtures under `tests/fixtures/adversary-w27-u1`: `loop.yaml` pings itself without end through
//! a binding; `unmet.yaml` records a note whose optional size is `big`, so a note without a size
//! leaves the binding's condition undecided; `chain-*.yaml` (differing only in `reached_by`) open
//! a case whose event two bindings match (`a-note`, `b-flag` where the case is urgent), and
//! the note's event a third (`c-touch`), which touches only a flagged case.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_kernel::Store;

const LOOP: &str = "crates/spindle-cli/tests/fixtures/adversary-w27-u1/loop.yaml";
const UNMET: &str = "crates/spindle-cli/tests/fixtures/adversary-w27-u1/unmet.yaml";
const CHAIN_IN_PROCESS: &str =
    "crates/spindle-cli/tests/fixtures/adversary-w27-u1/chain-in-process.yaml";
const CHAIN_NETWORK: &str = "crates/spindle-cli/tests/fixtures/adversary-w27-u1/chain-network.yaml";
const LINES: &str = "crates/spindle-cli/tests/fixtures/stdin-lines";
const TICKETS: &str = "crates/spindle-cli/tests/fixtures/stdin-bindings/in-process.yaml";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";
const LAPTOP: &str = "examples/environments/laptop.toml";

const BOOK: &str = "shelf.books.Book";
const ADD_BOOK: &str = "shelf.books.AddBook";
const LEND_BOOK: &str = "shelf.books.LendBook";
const MINTED: &str = "/events/0/payload/book_id";

/// How long a served `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(arguments: &[&str], lines: &[Value]) -> Output {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    run_text(arguments, &text)
}

fn run_text(arguments: &[&str], text: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let text = text.to_owned();
    // Written from its own thread: a run printing many lines must not block on a full pipe.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(text.as_bytes());
    });
    let output = child.wait_with_output().expect("b10x-spindle runs");
    let _ = writer.join();
    output
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn command(name: &str, input: &Value) -> Value {
    json!({"command": name, "input": input})
}

fn book(identity: &str, state: &str) -> Value {
    json!({"entity": BOOK, "identity": identity, "state": state, "fields": {"title": "Middlemarch"}})
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// The identities `count` `AddBook` lines mint, in order, on a fresh run seeded with `seed`.
fn drawn(seed: &str, count: usize) -> Vec<String> {
    let lines = vec![command(ADD_BOOK, &json!({"title": "Moby-Dick"})); count];
    let output = run(
        &["run", LINES, "--environment", LAPTOP, "--mint-seed", seed],
        &lines,
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    results(&output)
        .iter()
        .map(|result| {
            result["events"][0]["payload"]["book_id"]
                .as_str()
                .expect("a minted identity")
                .to_owned()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------------------------

/// A binding whose command emits the event that invokes it again stops after
/// `spindle_host::DELIVERY_LIMIT` bound commands: every one of them printed, then one `runaway`
/// line, and the run exits 1 after reading every line. Untested by the unit.
#[test]
fn a_runaway_binding_stops_at_the_delivery_limit_and_fails_the_run() {
    let started = Instant::now();
    let output = run(
        &["run", LOOP, "--environment", MEMORY],
        &[command("echo.ping.Ping", &json!({"n": 1}))],
    );
    let took = started.elapsed();
    assert_eq!(output.status.code(), Some(1), "{:?}", output.status);
    let results = results(&output);
    assert_eq!(
        results.len(),
        1 + spindle_host::DELIVERY_LIMIT + 1,
        "the line, every bound command up to the limit, and the runaway line"
    );
    assert_eq!(results[0]["outcome"], "pinged");
    assert!(
        results[1..=spindle_host::DELIVERY_LIMIT]
            .iter()
            .all(|result| result["outcome"] == "pinged"
                && result["caused_by"] == json!({"line": 1, "binding": "ping-again"})),
        "every bound command ran and names line 1"
    );
    let last = results.last().expect("a last line");
    assert_eq!(last["failure"]["kind"], "runaway", "{last}");
    assert_eq!(last["caused_by"]["binding"], "ping-again", "{last}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("line 1: its bindings kept invoking commands"),
        "{stderr}"
    );
    assert!(took < Duration::from_secs(60), "took {took:?}");
}

/// A binding whose `where` condition reads an optional field the event leaves absent is undecided:
/// it prints an `unmet_binding` line caused by the line, runs nothing and fails the run, while a
/// size `big` records the note and `small` invokes nothing. Untested by the unit.
#[test]
fn an_undecided_binding_condition_is_printed_and_fails_the_run() {
    let note = |id: &str, size: Option<&str>| {
        let mut input = json!({"note_id": id});
        if let Some(size) = size {
            input["size"] = json!(size);
        }
        command("memo.notes.Note", &input)
    };
    let output = run(
        &["run", UNMET, "--environment", MEMORY],
        &[
            note("n-1", Some("big")),
            note("n-2", Some("small")),
            note("n-3", None),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 5, "{results:?}");
    assert_eq!(results[1]["command"], "memo.notes.Record");
    assert_eq!(results[1]["outcome"], "recorded", "{}", results[1]);
    assert_eq!(results[2]["outcome"], "noted", "small invokes nothing");
    assert_eq!(results[3]["outcome"], "noted");
    assert_eq!(
        results[4]["failure"]["kind"], "unmet_binding",
        "{}",
        results[4]
    );
    assert_eq!(
        results[4]["caused_by"],
        json!({"line": 3, "binding": "record-large"})
    );
    assert_eq!(results[4]["attempts"], 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("line 3: binding record-large did not invoke"),
        "{stderr}"
    );
    assert!(!stderr.contains("line 1"), "{stderr}");
}

/// Two bindings on one event and a third on the event the first one's command emits run
/// breadth first, the two bindings of the line's event in binding-name order: `AddNote`,
/// `FlagCase`, then `TouchCase`, which finds the case flagged. A blank line before is counted in
/// `caused_by`.
#[test]
fn bound_commands_run_in_the_order_their_events_were_emitted() {
    let output = run_text(
        &[
            "run",
            CHAIN_IN_PROCESS,
            "--environment",
            LAPTOP,
            "--mint-seed",
            "11",
        ],
        &format!(
            "\n{}\n",
            command(
                "casework.cases.OpenCase",
                &json!({"case_id": "c-1", "urgency": "urgent"})
            )
        ),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    let order: Vec<(&str, &str, &str)> = results
        .iter()
        .map(|result| {
            (
                result["command"].as_str().unwrap_or_default(),
                result["outcome"].as_str().unwrap_or_default(),
                result["caused_by"]["binding"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            ("casework.cases.OpenCase", "opened", ""),
            ("casework.cases.AddNote", "added", "a-note"),
            ("casework.cases.FlagCase", "flagged", "b-flag"),
            ("casework.cases.TouchCase", "touched", "c-touch"),
        ]
    );
    for bound in &results[1..] {
        assert_eq!(bound["caused_by"]["line"], 2, "{bound}");
    }
}

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

fn serve(spec: &str, arguments: &[&str]) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec])
        .args(arguments)
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
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served { child, address }
}

/// The method and path ESS publishes for the command `id` of the served component of `spec`.
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

/// One HTTP/1.1 exchange on its own connection: the status.
fn send(spec: &str, address: SocketAddr, line: &Value) -> u16 {
    let (method, path) = route(spec, line["command"].as_str().expect("a command"));
    let body = line["input"].to_string();
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
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read before the timeout");
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"))
}

/// Every row of `entities` and every event, in commit order, in the SQLite store at `path`.
fn stored(path: &Path, entities: &[&str]) -> (Vec<Value>, Vec<Value>) {
    let mut store = spindle_kernel::SqliteStore::open(path).expect("the state opens");
    let mut rows = Vec::new();
    for entity in entities {
        for (identity, stored) in store.scan(entity).expect("the entity is read").rows {
            rows.push(json!([
                entity,
                identity,
                stored.row.state,
                stored.row.fields
            ]));
        }
    }
    let events = store
        .events_after(0, 10_000)
        .expect("the events are read")
        .into_iter()
        .map(|committed| json!([committed.event.name, committed.event.payload]))
        .collect();
    (rows, events)
}

/// A chain of bindings (two on one event, one with a `where` condition, one on a bound command's
/// event, a minted identity in a bound command, a refusal) leaves the same rows and the same
/// events in the same order on stdin and served, both seeded alike.
#[test]
fn a_chain_of_bindings_leaves_the_same_store_on_stdin_and_served() {
    let sequence = [
        command(
            "casework.cases.OpenCase",
            &json!({"case_id": "c-1", "urgency": "urgent"}),
        ),
        command(
            "casework.cases.OpenCase",
            &json!({"case_id": "c-2", "urgency": "routine"}),
        ),
        command(
            "casework.cases.OpenCase",
            &json!({"case_id": "c-3", "urgency": "urgent"}),
        ),
    ];
    let entities = ["casework.cases.Case", "casework.cases.Note"];
    let directory = scratch();

    let stdin_state = directory.path().join("stdin.sqlite");
    let output = run(
        &[
            "run",
            CHAIN_IN_PROCESS,
            "--environment",
            LAPTOP,
            "--mint-seed",
            "11",
            "--state",
            stdin_state.to_str().expect("a UTF-8 path"),
        ],
        &sequence,
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "c-2's touch is refused: {output:?}"
    );

    let served_state = directory.path().join("served");
    {
        let served = serve(
            CHAIN_NETWORK,
            &[
                "--environment",
                LAPTOP,
                "--mint-seed",
                "11",
                "--state",
                served_state.to_str().expect("a UTF-8 path"),
            ],
        );
        for line in &sequence {
            let status = send(CHAIN_NETWORK, served.address, line);
            assert!((200..300).contains(&status), "{line}: {status}");
        }
    }

    let on_stdin = stored(&stdin_state, &entities);
    let served = stored(&served_state.join("front.sqlite"), &entities);
    assert_eq!(on_stdin.0, served.0, "the same rows");
    assert_eq!(on_stdin.1, served.1, "the same events in the same order");
    assert_eq!(
        on_stdin.0.len(),
        6,
        "three cases and three notes: {:?}",
        on_stdin.0
    );
}

/// A specification without bindings: a rejected line's stderr is what it was at the base, byte
/// for byte (`story:run-fails-on-rejected-line`'s summary line).
#[test]
fn a_specification_without_bindings_summarises_a_rejected_line_as_before() {
    let output = run_text(&["run", "ess", "--environment", MEMORY], "not json\n");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(2).collect();
    assert_eq!(
        tail,
        [
            "b10x-spindle: 1 rejected of 1 commands read, so the run exits 1",
            "b10x-spindle: line 1 was rejected (invalid_request): it did not run",
        ],
        "{stderr}"
    );
}

// ---------------------------------------------------------------------------------------------
// Arrange lines and references
// ---------------------------------------------------------------------------------------------

/// A seeded run that minted once, then arranges the seed's second draw, mints the third next: the
/// mint keeps its place in the sequence when an arrange line adds to what it avoids.
#[test]
fn an_arrange_line_mid_run_keeps_the_seeded_sequence() {
    let draws = drawn("7", 3);
    let output = run(
        &["run", LINES, "--environment", LAPTOP, "--mint-seed", "7"],
        &[
            command(ADD_BOOK, &json!({"title": "One"})),
            json!({"arrange": [book(&draws[1], "Shelved")]}),
            command(ADD_BOOK, &json!({"title": "Two"})),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[0]["events"][0]["payload"]["book_id"], draws[0]);
    assert_eq!(
        results[2]["events"][0]["payload"]["book_id"], draws[2],
        "{}",
        results[2]
    );
}

/// An arrange line naming an identity an earlier line minted is refused (`already stored`) and
/// changes nothing: the minted book stays lent.
#[test]
fn an_arrange_line_over_a_minted_identity_is_rejected() {
    let draws = drawn("7", 1);
    let output = run(
        &["run", LINES, "--environment", LAPTOP, "--mint-seed", "7"],
        &[
            command(ADD_BOOK, &json!({"title": "One"})),
            command(
                LEND_BOOK,
                &json!({"book_id": {"$line": 1, "pointer": MINTED}}),
            ),
            json!({"arrange": [book(&draws[0], "Shelved")]}),
            command(
                LEND_BOOK,
                &json!({"book_id": {"$line": 1, "pointer": MINTED}}),
            ),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results[1]["outcome"], "lent");
    assert_eq!(
        results[2]["failure"]["kind"], "invalid_arrangement",
        "{}",
        results[2]
    );
    assert_eq!(results[3]["outcome"], "already-lent", "{}", results[3]);
}

/// A row that breaks its entity's invariant (`copies <= 9`) is refused, nothing is written.
#[test]
fn an_arrange_row_breaking_an_invariant_is_rejected() {
    let output = run(
        &["run", TICKETS, "--environment", MEMORY],
        &[
            json!({"arrange": [{"entity": "desk.tickets.Filing", "identity": "t-9", "state": "Filed", "fields": {"copies": 12}}]}),
            command("desk.tickets.ArchiveFiling", &json!({"ticket_id": "t-9"})),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(
        results[0]["failure"]["kind"], "invalid_arrangement",
        "{}",
        results[0]
    );
    assert_eq!(results[1]["outcome"], "no-such-filing", "{}", results[1]);
}

/// A reference may name an arrange line's answer, and blank lines count: line 3 lends the book
/// line 2 arranged.
#[test]
fn a_reference_names_an_arranged_identity_counting_blank_lines() {
    let first = "6f1d3c1e-0000-4000-8000-0000000000aa";
    let output = run_text(
        &["run", LINES, "--environment", LAPTOP],
        &format!(
            "\n{}\n{}\n",
            json!({"arrange": [book(first, "Shelved")]}),
            command(
                LEND_BOOK,
                &json!({"book_id": {"$line": 2, "pointer": "/arranged/0/identity"}})
            )
        ),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[1]["outcome"], "lent", "{}", results[1]);
    assert_eq!(results[1]["events"][0]["payload"]["book_id"], first);
}

/// References that are malformed (a negative or string line, an extra member, no pointer) or name
/// the line itself reject their line with `unresolved_reference`; a value that is no reference
/// passes untouched.
#[test]
fn a_malformed_reference_rejects_its_line() {
    let lend = |reference: Value| command(LEND_BOOK, &json!({"book_id": reference}));
    let output = run(
        &["run", LINES, "--environment", LAPTOP],
        &[
            command(ADD_BOOK, &json!({"title": "One"})),
            lend(json!({"$line": -1, "pointer": MINTED})),
            lend(json!({"$line": "1", "pointer": MINTED})),
            lend(json!({"$line": 1, "pointer": MINTED, "extra": true})),
            lend(json!({"$line": 1})),
            lend(json!({"$line": 6, "pointer": MINTED})),
            lend(json!({"$line": 0, "pointer": MINTED})),
            lend(json!({"$line": 1, "pointer": MINTED})),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    for (index, result) in results.iter().enumerate().take(7).skip(1) {
        assert_eq!(
            result["failure"]["kind"],
            "unresolved_reference",
            "line {}: {result}",
            index + 1
        );
    }
    assert_eq!(results[7]["outcome"], "lent", "{}", results[7]);
}

const RELAY_IN_PROCESS: &str =
    "crates/spindle-cli/tests/fixtures/adversary-w27-u1/relay-in-process";
const RELAY_NETWORK: &str = "crates/spindle-cli/tests/fixtures/adversary-w27-u1/relay-network";

/// Every row of `entity` in the SQLite store at `path`: identity and fields.
fn rows(path: &Path, entity: &str) -> Vec<Value> {
    let mut store = spindle_kernel::SqliteStore::open(path).expect("the state opens");
    store
        .scan(entity)
        .expect("the entity is read")
        .rows
        .into_iter()
        .map(|(identity, stored)| json!([identity, stored.row.fields]))
        .collect()
}

/// One event two bindings match, one leading to another component (`a-zulu`), one back to the
/// line's own (`b-alpha`), each bound command minting an identity: a seeded stdin run and a
/// seeded served run of the same specification (differing only in `alpha`'s `reached_by`) mint
/// the same identity for the same row. The served host delivers by link, `(alpha, alpha)` before
/// `(alpha, zulu)`; the stdin run by binding name.
#[test]
fn bindings_to_two_components_mint_alike_on_stdin_and_served() {
    let open = command("relay.alpha.Open", &json!({"case_id": "c-1"}));
    let directory = scratch();
    let stdin_state = directory.path().join("stdin.sqlite");
    let output = run(
        &[
            "run",
            RELAY_IN_PROCESS,
            "--environment",
            LAPTOP,
            "--mint-seed",
            "5",
            "--state",
            stdin_state.to_str().expect("a UTF-8 path"),
        ],
        std::slice::from_ref(&open),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    let served_state = directory.path().join("served");
    {
        let served = serve(
            RELAY_NETWORK,
            &[
                "--environment",
                LAPTOP,
                "--mint-seed",
                "5",
                "--state",
                served_state.to_str().expect("a UTF-8 path"),
            ],
        );
        let status = send(RELAY_NETWORK, served.address, &open);
        assert!((200..300).contains(&status), "{status}");
    }

    let stdin_memos = rows(&stdin_state, "relay.alpha.Memo");
    let stdin_copies = rows(&stdin_state, "relay.zulu.Copy");
    let served_memos = rows(&served_state.join("alpha.sqlite"), "relay.alpha.Memo");
    let served_copies = rows(&served_state.join("zulu.sqlite"), "relay.zulu.Copy");
    assert_eq!(stdin_memos.len(), 1, "{stdin_memos:?}");
    assert_eq!(stdin_copies.len(), 1, "{stdin_copies:?}");
    assert_eq!(
        (stdin_memos, stdin_copies),
        (served_memos, served_copies),
        "the same memo and the same copy, identities included; stdin printed {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
