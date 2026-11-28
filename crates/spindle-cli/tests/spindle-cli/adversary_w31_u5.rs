//! Adversary of wave 31 U5 (`story:spec-change-over-stored-rows`), round 2: a served run of two
//! components whose domains swap owners. `tests/fixtures/adversary-w31-u5-moved-before` has
//! `archive` own the books and `vault` the coins; `adversary-w31-u5-moved-after` swaps them and
//! retypes the book's title. The books stored in `archive`'s file are not in `vault`'s, so the run
//! exits 3 before listening, naming the book and the component it moved to, and leaves the file
//! and its record as they were.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_kernel::{Change, Fit, Row, Shapes, SqliteStore, Store};

const BEFORE: &str = "crates/spindle-cli/tests/fixtures/adversary-w31-u5-moved-before";
const AFTER: &str = "crates/spindle-cli/tests/fixtures/adversary-w31-u5-moved-after";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BOOK: &str = "archive.shelf.Book";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir(spec: &str) -> spindle_spec::EssIr {
    spindle_spec::load(&root().join(spec))
        .unwrap_or_else(|error| panic!("{spec} compiles: {error}"))
        .ir
}

/// `archive`'s file in `state` as a run under [`BEFORE`] leaves it after shelving one book.
fn shelved(file: &Path) -> Shapes {
    let recorded = Shapes::owned(&ir(BEFORE), "archive");
    let mut store = SqliteStore::open(file).expect("the store opens");
    assert_eq!(
        store.fit(&recorded).expect("checked"),
        Fit::Fits { unrecorded: false }
    );
    store
        .commit(&Change {
            entity: BOOK.to_owned(),
            identity: json!("b1"),
            expected: None,
            row: Row {
                state: "Shelved".to_owned(),
                fields: BTreeMap::from([("title".to_owned(), json!("Dune"))]),
            },
            events: vec![spindle_kernel::Event {
                name: "archive.shelf.BookShelved".to_owned(),
                payload: serde_json::Map::from_iter([("id".to_owned(), json!("b1"))]),
            }],
            scanned: Vec::new(),
        })
        .expect("the book is shelved");
    recorded
}

#[test]
fn a_domain_moved_between_served_components_exits_3_naming_the_new_owner() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let state = scratch.path().join("state");
    std::fs::create_dir_all(&state).expect("the state directory");
    let file = state.join("archive.sqlite");
    let recorded = shelved(&file);
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", AFTER, "--environment", LAPTOP, "--state"])
        .arg(&state)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            text.push_str(&line);
            text.push('\n');
            if line.contains("\"listening\"") {
                break;
            }
        }
        let _ = sender.send(text);
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the run is waited on") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "the run neither exited nor was refused within 30 s: {}",
                receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let printed = receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    assert_eq!(status.code(), Some(3), "{printed}");
    assert!(
        printed.contains(BOOK)
            && printed.contains("moved to component vault")
            && !printed.contains("removed")
            && !printed.contains("listening"),
        "stdout names the book and its new owner: {printed}"
    );
    let mut store = SqliteStore::open(&file).expect("the store opens");
    assert_eq!(
        store.recorded().expect("the record reads"),
        Some(recorded),
        "the record is unchanged"
    );
    let books: Vec<(Value, Value)> = store
        .scan(BOOK)
        .expect("the books read")
        .into_iter()
        .map(|(identity, stored)| (identity, json!(stored.row.fields)))
        .collect();
    assert_eq!(books, vec![(json!("b1"), json!({"title": "Dune"}))]);
}
