//! `sets: {f: {append: input.<path>}}` (ess/31): the list the instance held in `f` before the
//! outcome, an absent `Optional<List<…>>` read as empty, with the input element last, as ESS's
//! interpreter determines it (`interpret::execute::determined::append`).
//!
//! The fixture `tests/fixtures/list_append` opens a notebook with no pages
//! (`Optional<List<String>>`, absent) and an empty list of margin notes
//! (`List<Optional<String>>`, `[]`). Adding a page appends a required `String`; adding a margin
//! appends an `Optional<String>`, `null` where none is sent. The view `NotebookCard` reading both
//! lists whole is `crates/spindle-host/tests/spindle-host/list_append.rs`.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, Store, execute};

const NOTEBOOK: &str = "scribe.notebook.Notebook";
const NOTEBOOK_ID: &str = "4d1c7a2e-9b3f-4c5d-8e6f-0a1b2c3d4e5f";

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/list_append")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the list_append fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

fn run(store: &mut MemoryStore, command: &str, sent: Value) {
    let ran = execute(&ir(), store, &mut NoExternals, command, &input(sent))
        .unwrap_or_else(|error| panic!("{command} runs: {error}"));
    assert_eq!(ran.error, None, "{command} is not refused");
}

fn open(store: &mut MemoryStore) {
    run(
        store,
        "scribe.notebook.OpenNotebook",
        json!({"notebook_id": NOTEBOOK_ID, "title": "field notes"}),
    );
}

fn add_page(store: &mut MemoryStore, page: &str) {
    run(
        store,
        "scribe.notebook.AddPage",
        json!({"notebook_id": NOTEBOOK_ID, "page": page}),
    );
}

fn field(store: &mut MemoryStore, name: &str) -> Option<Value> {
    store
        .load(NOTEBOOK, &json!(NOTEBOOK_ID))
        .expect("the store reads")
        .expect("the notebook is stored")
        .row
        .fields
        .get(name)
        .cloned()
}

#[test]
fn appending_to_an_absent_list_stores_one_element() {
    let mut store = MemoryStore::default();
    open(&mut store);
    assert_eq!(
        field(&mut store, "pages"),
        None,
        "a notebook opens without pages"
    );
    add_page(&mut store, "dawn chorus");
    assert_eq!(
        field(&mut store, "pages"),
        Some(json!(["dawn chorus"])),
        "an absent `Optional<List<String>>` is read as empty, so one page is stored"
    );
}

#[test]
fn appending_keeps_the_previous_elements_in_order() {
    let mut store = MemoryStore::default();
    open(&mut store);
    for page in ["dawn chorus", "low tide", "first frost"] {
        add_page(&mut store, page);
    }
    assert_eq!(
        field(&mut store, "pages"),
        Some(json!(["dawn chorus", "low tide", "first frost"])),
        "each page follows every page held before it; the newest is last"
    );
    assert_eq!(
        field(&mut store, "title"),
        Some(json!("field notes")),
        "an append writes only its own field"
    );
}

#[test]
fn appending_to_a_stored_empty_list_and_an_absent_optional_element_appends_null() {
    let mut store = MemoryStore::default();
    open(&mut store);
    assert_eq!(field(&mut store, "margins"), Some(json!([])));
    run(
        &mut store,
        "scribe.notebook.AddMargin",
        json!({"notebook_id": NOTEBOOK_ID, "note": "see p. 2"}),
    );
    run(
        &mut store,
        "scribe.notebook.AddMargin",
        json!({"notebook_id": NOTEBOOK_ID}),
    );
    assert_eq!(
        field(&mut store, "margins"),
        Some(json!(["see p. 2", null])),
        "over a `List<Optional<String>>` a note not sent appends `null`, as ESS's interpreter \
         and generated Rust and Go do"
    );
}
