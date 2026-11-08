//! Adversary cases for `story:list-append-values`, over the neutral fixture
//! `tests/fixtures/adversary_append`: a shelf with an `Optional<List<String>>` of tags under the
//! invariant `tags.count <= 3`, a `List<Optional<String>>` of slots, an `Optional<List<Entry>>` of
//! struct entries and an `Optional<List<Optional<String>>>` of marks.
//!
//! Each expected value is the one ESS 0.86.0's interpreted target holds for the same model: the
//! authored scenarios behind these cases pass `ess verify conform run --target interpreted`, and
//! the invariant case is an `ESS-CF-TARGET` error there ("the model's own outcome leaves
//! `probe.shelf.Shelf` ... violating its invariant `tags.count <= 3`").

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Row, SqliteStore, Store, execute,
};

const SHELF: &str = "probe.shelf.Shelf";
const ID: &str = "4d1c7a2e-9b3f-4c5d-8e6f-0a1b2c3d4e5f";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_append");
    spindle_spec::load(&root)
        .expect("the adversary_append fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

fn run(store: &mut dyn Store, command: &str, sent: Value) -> Result<Executed, ExecuteError> {
    execute(&ir(), store, &mut NoExternals, command, &input(sent))
}

fn ok(store: &mut dyn Store, command: &str, sent: Value) {
    let ran = run(store, command, sent).unwrap_or_else(|error| panic!("{command} runs: {error}"));
    assert_eq!(ran.error, None, "{command} is not refused");
}

fn open(store: &mut dyn Store) {
    ok(
        store,
        "probe.shelf.OpenShelf",
        json!({"shelf_id": ID, "label": "l"}),
    );
}

fn fields(store: &mut dyn Store) -> BTreeMap<String, Value> {
    store
        .load(SHELF, &json!(ID))
        .expect("the store reads")
        .expect("the shelf is stored")
        .row
        .fields
}

#[test]
fn an_element_read_through_a_struct_input_path_is_appended() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.AddWrapped",
        json!({"shelf_id": ID, "wrap": {"text": "w"}}),
    );
    assert_eq!(fields(&mut store).get("tags"), Some(&json!(["w"])));
}

#[test]
fn an_absent_optional_struct_on_the_path_appends_null_to_a_list_of_optionals() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.AddSlotFrom",
        json!({"shelf_id": ID}),
    );
    ok(
        &mut store,
        "probe.shelf.AddSlotFrom",
        json!({"shelf_id": ID, "wrap": null}),
    );
    ok(
        &mut store,
        "probe.shelf.AddSlotFrom",
        json!({"shelf_id": ID, "wrap": {"text": "s"}}),
    );
    assert_eq!(
        fields(&mut store).get("slots"),
        Some(&json!([null, null, "s"]))
    );
}

#[test]
fn an_explicit_null_element_over_an_absent_optional_list_of_optionals_stores_one_null() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.AddMark",
        json!({"shelf_id": ID, "mark": null}),
    );
    ok(
        &mut store,
        "probe.shelf.AddMark",
        json!({"shelf_id": ID, "mark": "m"}),
    );
    assert_eq!(fields(&mut store).get("marks"), Some(&json!([null, "m"])));
}

#[test]
fn a_struct_element_is_appended_whole() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.AddEntry",
        json!({"shelf_id": ID, "entry": {"text": "e", "note": "n"}}),
    );
    ok(
        &mut store,
        "probe.shelf.AddEntry",
        json!({"shelf_id": ID, "entry": {"text": "f"}}),
    );
    assert_eq!(
        fields(&mut store).get("entries"),
        Some(&json!([{"text": "e", "note": "n"}, {"text": "f"}]))
    );
}

#[test]
fn an_append_beside_another_sets_entry_writes_both_from_the_same_input() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.AddTag",
        json!({"shelf_id": ID, "tag": "a"}),
    );
    ok(
        &mut store,
        "probe.shelf.TagAndRename",
        json!({"shelf_id": ID, "tag": "r"}),
    );
    let held = fields(&mut store);
    assert_eq!(held.get("tags"), Some(&json!(["a", "r"])));
    assert_eq!(held.get("label"), Some(&json!("r")));
}

#[test]
fn an_append_that_breaks_the_entity_invariant_is_an_error_and_writes_nothing() {
    let mut store = MemoryStore::default();
    open(&mut store);
    for tag in ["a", "b", "c"] {
        ok(
            &mut store,
            "probe.shelf.AddTag",
            json!({"shelf_id": ID, "tag": tag}),
        );
    }
    let fourth = run(
        &mut store,
        "probe.shelf.AddTag",
        json!({"shelf_id": ID, "tag": "d"}),
    );
    assert!(
        matches!(fourth, Err(ExecuteError::BrokenInvariant { .. })),
        "ESS's interpreter refuses the step: the outcome would break `tags.count <= 3`; got \
         {fourth:?}"
    );
    assert_eq!(
        fields(&mut store).get("tags"),
        Some(&json!(["a", "b", "c"]))
    );
}

#[test]
fn a_wrong_state_refusal_appends_nothing() {
    let mut store = MemoryStore::default();
    open(&mut store);
    ok(
        &mut store,
        "probe.shelf.SealWithTag",
        json!({"shelf_id": ID, "tag": "last"}),
    );
    let again = run(
        &mut store,
        "probe.shelf.SealWithTag",
        json!({"shelf_id": ID, "tag": "late"}),
    )
    .expect("the refusal runs");
    assert_eq!(
        again.error.map(|error| error.name),
        Some("probe.shelf.NotOpen".to_owned())
    );
    let held = store
        .load(SHELF, &json!(ID))
        .expect("the store reads")
        .expect("the shelf is stored");
    assert_eq!(held.row.state, "Sealed");
    assert_eq!(held.row.fields.get("tags"), Some(&json!(["last"])));
}

#[test]
fn an_element_of_the_wrong_type_is_refused_before_anything_is_written() {
    let mut store = MemoryStore::default();
    open(&mut store);
    let wrong = run(
        &mut store,
        "probe.shelf.AddTag",
        json!({"shelf_id": ID, "tag": 5}),
    );
    assert!(
        matches!(wrong, Err(ExecuteError::InvalidInput(_))),
        "got {wrong:?}"
    );
    let entry = run(
        &mut store,
        "probe.shelf.AddEntry",
        json!({"shelf_id": ID, "entry": {"text": "e", "extra": 1}}),
    );
    assert!(
        matches!(entry, Err(ExecuteError::InvalidInput(_))),
        "got {entry:?}"
    );
    let held = fields(&mut store);
    assert_eq!(held.get("tags"), None);
    assert_eq!(held.get("entries"), None);
}

#[test]
fn a_held_null_reads_as_empty_for_an_optional_and_a_required_list() {
    let mut store = MemoryStore::default();
    open(&mut store);
    let stored = store
        .load(SHELF, &json!(ID))
        .expect("the store reads")
        .expect("the shelf is stored");
    let mut fields_held = stored.row.fields.clone();
    fields_held.insert("tags".to_owned(), Value::Null);
    fields_held.insert("slots".to_owned(), Value::Null);
    store
        .commit(&Change {
            entity: SHELF.to_owned(),
            identity: json!(ID),
            expected: Some(stored.version),
            row: Row {
                state: stored.row.state.clone(),
                fields: fields_held,
            },
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("the held nulls commit");
    ok(
        &mut store,
        "probe.shelf.AddTag",
        json!({"shelf_id": ID, "tag": "a"}),
    );
    ok(
        &mut store,
        "probe.shelf.AddSlotFrom",
        json!({"shelf_id": ID, "wrap": {"text": "s"}}),
    );
    let held = fields(&mut store);
    assert_eq!(held.get("tags"), Some(&json!(["a"])));
    assert_eq!(held.get("slots"), Some(&json!(["s"])));
}

#[test]
fn a_long_list_keeps_every_element_in_order_on_memory() {
    let mut store = MemoryStore::default();
    open(&mut store);
    let count = 2000;
    for index in 0..count {
        ok(
            &mut store,
            "probe.shelf.AddMark",
            json!({"shelf_id": ID, "mark": index.to_string()}),
        );
    }
    let expected: Vec<Value> = (0..count).map(|index| json!(index.to_string())).collect();
    assert_eq!(
        fields(&mut store).get("marks"),
        Some(&Value::Array(expected))
    );
}

#[test]
fn a_list_past_one_mebibyte_is_appended_and_read_back_on_sqlite() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("shelf.sqlite");
    let mut store = SqliteStore::open(&path).expect("the store opens");
    open(&mut store);
    let element = "x".repeat(64 * 1024);
    let count = 20;
    for _ in 0..count {
        ok(
            &mut store,
            "probe.shelf.AddMark",
            json!({"shelf_id": ID, "mark": element}),
        );
    }
    drop(store);
    let mut reopened = SqliteStore::open(&path).expect("the store reopens");
    let marks = fields(&mut reopened)
        .get("marks")
        .cloned()
        .expect("the marks are held");
    assert_eq!(marks.as_array().map(Vec::len), Some(count));
    assert!(
        marks
            .as_array()
            .is_some_and(|items| items.iter().all(|item| item == &json!(element)))
    );
}
