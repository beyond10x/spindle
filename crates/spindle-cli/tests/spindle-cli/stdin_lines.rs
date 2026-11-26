//! `run` reading JSON lines on stdin takes two more kinds of line beside a command
//! (story:stdin-arrange-and-line-references): `{"arrange": [<row>]}` writes stored rows in the
//! reset world's shape before the next line, and a command's input may name a value of an earlier
//! line's answer with `{"$line": <n>, "pointer": "<JSON pointer>"}`.
//!
//! The fixture `tests/fixtures/stdin-lines` declares `shelf.books.Book`, shelved, lent or
//! withdrawn: `AddBook` mints the new book's identity and publishes it in `BookAdded`; `LendBook`,
//! `ReturnBook` and `Withdraw` are guarded on the book's state, and `Withdraw` is granted only to
//! the system actor `shelf.books.Librarian`. The environment is the laptop's, whose store is
//! `SQLite` in a temporary directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/stdin-lines";
const LAPTOP: &str = "examples/environments/laptop.toml";

const BOOK: &str = "shelf.books.Book";
const ADD_BOOK: &str = "shelf.books.AddBook";
const LEND_BOOK: &str = "shelf.books.LendBook";
const RETURN_BOOK: &str = "shelf.books.ReturnBook";
const WITHDRAW: &str = "shelf.books.Withdraw";
const LIBRARIAN: &str = "shelf.books.Librarian";

const FIRST: &str = "6f1d3c1e-0000-4000-8000-000000000001";
const SECOND: &str = "6f1d3c1e-0000-4000-8000-000000000002";
const MINTED: &str = "/events/0/payload/book_id";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(extra: &[&str], lines: &[Value]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes())
        .expect("the lines are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn book(identity: &str, state: &str) -> Value {
    json!({"entity": BOOK, "identity": identity, "state": state, "fields": {"title": "Middlemarch"}})
}

fn command(name: &str, input: &Value) -> Value {
    json!({"command": name, "input": input})
}

fn reference(line: u64, pointer: &str) -> Value {
    json!({"$line": line, "pointer": pointer})
}

/// A row arranged lent is returned: `ReturnBook` answers its guarded outcome.
#[test]
fn an_arrange_line_writes_rows_a_later_command_reads() {
    let output = run(
        &[],
        &[
            json!({"arrange": [book(FIRST, "Lent")]}),
            command(RETURN_BOOK, &json!({"book_id": FIRST})),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(
        results[0]["arranged"],
        json!([{"entity": BOOK, "identity": FIRST}])
    );
    assert_eq!(results[1]["outcome"], "returned", "{}", results[1]);
}

/// The identity line 1 minted is named by line 2 through a reference to line 1's answer.
#[test]
fn a_line_names_an_identity_an_earlier_line_minted() {
    let output = run(
        &[],
        &[
            command(ADD_BOOK, &json!({"title": "Moby-Dick"})),
            command(LEND_BOOK, &json!({"book_id": reference(1, MINTED)})),
            command(RETURN_BOOK, &json!({"book_id": reference(1, MINTED)})),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[0]["outcome"], "added", "{}", results[0]);
    assert!(results[0]["events"][0]["payload"]["book_id"].is_string());
    assert_eq!(results[1]["outcome"], "lent", "{}", results[1]);
    assert_eq!(
        results[1]["events"][0]["payload"]["book_id"],
        results[0]["events"][0]["payload"]["book_id"],
        "line 2 lent the book line 1 added"
    );
    assert_eq!(results[2]["outcome"], "returned", "{}", results[2]);
}

/// A reference to a later line, to a line that does not exist, or to a pointer the earlier answer
/// lacks rejects its line; the run reads every line and exits 1.
#[test]
fn a_reference_to_a_line_not_yet_answered_is_rejected() {
    let output = run(
        &[],
        &[
            command(LEND_BOOK, &json!({"book_id": reference(2, MINTED)})),
            command(ADD_BOOK, &json!({"title": "Moby-Dick"})),
            command(LEND_BOOK, &json!({"book_id": reference(9, MINTED)})),
            command(
                LEND_BOOK,
                &json!({"book_id": reference(2, "/events/0/payload/shelf")}),
            ),
            command(LEND_BOOK, &json!({"book_id": reference(2, MINTED)})),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 5, "every line is answered: {results:?}");
    for rejected in [0, 2, 3] {
        assert_eq!(
            results[rejected]["failure"]["kind"],
            "unresolved_reference",
            "line {}: {}",
            rejected + 1,
            results[rejected]
        );
        assert_eq!(results[rejected]["command"], LEND_BOOK);
    }
    assert_eq!(results[1]["outcome"], "added");
    assert_eq!(results[4]["outcome"], "lent", "{}", results[4]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in ["line 1 ", "line 3 ", "line 4 "] {
        assert!(stderr.contains(line), "stderr names {line}: {stderr}");
    }
}

/// A row with a field the entity does not declare rejects the whole arrange line: its other row
/// is not written either.
#[test]
fn an_arrange_line_refused_by_the_schema_is_rejected() {
    let mut coloured = book(SECOND, "Shelved");
    coloured["fields"]["colour"] = json!("red");
    let output = run(
        &[],
        &[
            json!({"arrange": [book(FIRST, "Shelved"), coloured]}),
            command(LEND_BOOK, &json!({"book_id": FIRST})),
            command(LEND_BOOK, &json!({"book_id": SECOND})),
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(
        results[0]["failure"]["kind"], "invalid_arrangement",
        "{}",
        results[0]
    );
    let message = results[0]["failure"]["message"]
        .as_str()
        .unwrap_or_default();
    assert!(
        message.contains("colour"),
        "the message names the field: {message}"
    );
    assert_eq!(results[1]["outcome"], "no-such-book", "nothing was written");
    assert_eq!(results[2]["outcome"], "no-such-book", "nothing was written");
}

/// With `--mint-seed`, a row arranged under the identity the seed draws first is not minted
/// again: the next `AddBook` mints another identity and answers `added`.
#[test]
fn a_seeded_mint_avoids_an_arranged_identity() {
    let seeded = ["--mint-seed", "7"];
    let first = run(
        &seeded,
        &[command(ADD_BOOK, &json!({"title": "Moby-Dick"}))],
    );
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let drawn = results(&first)[0]["events"][0]["payload"]["book_id"]
        .as_str()
        .expect("the seed's first identity")
        .to_owned();

    let output = run(
        &seeded,
        &[
            json!({"arrange": [book(&drawn, "Shelved")]}),
            command(ADD_BOOK, &json!({"title": "Moby-Dick"})),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[1]["outcome"], "added", "{}", results[1]);
    let minted = results[1]["events"][0]["payload"]["book_id"]
        .as_str()
        .expect("a minted identity");
    assert_ne!(minted, drawn, "the arranged identity is not minted again");
}

/// A command granted only to a system actor, sent on stdin as that actor, answers as it did
/// before arrange lines and references.
#[test]
fn a_system_actor_command_on_stdin_still_answers() {
    let output = run(
        &[],
        &[
            command(ADD_BOOK, &json!({"title": "Moby-Dick"})),
            json!({
                "command": WITHDRAW,
                "actor": LIBRARIAN,
                "input": {"book_id": reference(1, MINTED)},
            }),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results[1]["command"], WITHDRAW);
    assert_eq!(results[1]["outcome"], "withdrawn", "{}", results[1]);
}
