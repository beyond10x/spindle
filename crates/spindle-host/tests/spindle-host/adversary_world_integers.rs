//! Adversary cases for `story:world-row-integer-fields`: a row a reset's world loads
//! (`spindle_host::read_world`) holds its identity and fields as the world spells them. A world
//! row must answer what a command-created row of the same values answers, and two spellings ESS
//! reads as one value (`1` and `1.0` of an `Integer`) must name one instance.
//!
//! The model, inline: shelves with an `Integer` identity, and books that reference a shelf by an
//! `Integer`, with `Integer`, `Decimal`, `Optional<Integer>`, `List<Integer>`, struct and
//! `Timestamp` fields.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Answers, Host, ManualClock};

const SYSTEM: &str = r"format: ess/34
system: probe
version: v1
summary: Shelves keyed by an integer, and books on them.
domains:
  - probe.ints
components:
  - component: library
    summary: Keeps shelves and books.
    owns:
      domains:
        - probe.ints
    accepts:
      commands:
        - probe.ints.AddShelf
        - probe.ints.RenameShelf
        - probe.ints.AddBook
    publishes:
      events:
        - probe.ints.ShelfAdded
        - probe.ints.ShelfRenamed
        - probe.ints.BookAdded
    reached_by: in_process
";

const DOMAIN: &str = r"domain: probe.ints
summary: Shelves keyed by an integer, and books on them.
types:
  - name: probe.ints.Size
    kind: struct
    fields:
      - name: width
        type: Integer
entities:
  - name: probe.ints.Shelf
    identity:
      name: shelf_id
      type: Integer
    fields:
      - name: label
        type: String
    lifecycle:
      initial: Open
      states: [Open]
      terminal: [Open]
  - name: probe.ints.Book
    identity:
      name: book_id
      type: String
    fields:
      - name: shelf_id
        type: Optional<Integer>
      - name: pages
        type: Integer
      - name: weight
        type: Decimal
      - name: spare
        type: Optional<Integer>
      - name: counts
        type: List<Integer>
      - name: size
        type: probe.ints.Size
      - name: printed
        type: Timestamp
    relations:
      - {name: shelf, kind: references, target: probe.ints.Shelf, cardinality: one, via: shelf_id}
    lifecycle:
      initial: Kept
      states: [Kept]
      terminal: [Kept]
commands:
  - name: probe.ints.AddShelf
    input:
      - name: shelf_id
        type: Integer
      - name: label
        type: String
    outcomes:
      - name: added
        creates: probe.ints.Shelf
        instance: shelf_id
        sets:
          label: input.label
        emits: [probe.ints.ShelfAdded]
        payload:
          probe.ints.ShelfAdded: {shelf_id: input.shelf_id}
        summary: The shelf is kept.
  - name: probe.ints.RenameShelf
    input:
      - name: shelf_id
        type: Integer
      - name: label
        type: String
    outcomes:
      - name: renamed
        updates: probe.ints.Shelf
        instance: shelf_id
        sets:
          label: input.label
        emits: [probe.ints.ShelfRenamed]
        payload:
          probe.ints.ShelfRenamed: {shelf_id: input.shelf_id}
        summary: The shelf carries the new label.
  - name: probe.ints.AddBook
    input:
      - name: book_id
        type: String
      - name: shelf_id
        type: Optional<Integer>
      - name: pages
        type: Integer
      - name: weight
        type: Decimal
      - name: spare
        type: Optional<Integer>
      - name: counts
        type: List<Integer>
      - name: size
        type: probe.ints.Size
      - name: printed
        type: Timestamp
    outcomes:
      - name: added
        creates: probe.ints.Book
        instance: book_id
        sets:
          shelf_id: input.shelf_id
          pages: input.pages
          weight: input.weight
          spare: input.spare
          counts: input.counts
          size: input.size
          printed: input.printed
        emits: [probe.ints.BookAdded]
        payload:
          probe.ints.BookAdded: {book_id: input.book_id}
        summary: The book is kept.
events:
  - name: probe.ints.ShelfAdded
    fields:
      - name: shelf_id
        type: Integer
  - name: probe.ints.ShelfRenamed
    fields:
      - name: shelf_id
        type: Integer
  - name: probe.ints.BookAdded
    fields:
      - name: book_id
        type: String
views:
  - name: probe.ints.Shelves
    source: probe.ints.Shelf
    consistency: read_your_writes
    fields:
      - name: shelf_id
        type: Integer
      - name: label
        type: String
  - name: probe.ints.Books
    source: probe.ints.Book
    consistency: read_your_writes
    fields:
      - name: book_id
        type: String
      - name: pages
        type: Integer
      - name: weight
        type: Decimal
      - name: spare
        type: Optional<Integer>
      - name: counts
        type: List<Integer>
      - name: size
        type: probe.ints.Size
      - name: printed
        type: Timestamp
      - name: shelf_label
        type: Optional<String>
        field: {related: {via: shelf_id, field: label}}
  - name: probe.ints.OnePage
    source: probe.ints.Book
    consistency: read_your_writes
    filter: pages == 1
    order_by: [pages desc]
    fields:
      - name: book_id
        type: String
      - name: pages
        type: Integer
";

const SHELVES: &str = "probe.ints.Shelves";
const BOOKS: &str = "probe.ints.Books";

fn ir() -> ess_compiler::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/ints.yaml".to_owned()],
        &[SYSTEM.to_owned(), DOMAIN.to_owned()],
    )
    .expect("the inline model compiles")
}

fn memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"ints\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the environment parses")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

struct Opened {
    host: Host,
    _state: tempfile::TempDir,
}

fn host() -> Opened {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let host = Host::new(ir(), &memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host opens: {error}"));
    Opened {
        host,
        _state: state,
    }
}

/// `world` read by `read_world` and loaded by a reset.
fn reset(host: &mut Host, world: &Value) -> Result<(), String> {
    let ir = ir();
    let world = spindle_host::read_world(&ir, world, |table| table.over(Answers::default()))?;
    host.reset_to(world)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn listed(host: &mut Host, view: &str) -> Vec<Map<String, Value>> {
    host.listed_with(view, &Map::new())
        .unwrap_or_else(|error| panic!("{view} lists: {error}"))
        .rows
}

fn book(book_id: &str, fields: &Value) -> Value {
    json!({"entity": "probe.ints.Book", "identity": book_id, "state": "Kept", "fields": fields})
}

fn shelf(identity: Value, label: &str) -> Value {
    json!({"entity": "probe.ints.Shelf", "identity": identity, "state": "Open",
           "fields": {"label": label}})
}

/// The Books row a world loads with `fields`, and the one `AddBook` creates with the same
/// values, both listed.
fn world_and_command(fields: &Value) -> (Map<String, Value>, Map<String, Value>) {
    let mut world = host();
    reset(&mut world.host, &json!({"rows": [book("b1", fields)]}))
        .unwrap_or_else(|why| panic!("the world loads: {why}"));
    let from_world = listed(&mut world.host, BOOKS).remove(0);

    let mut command = host();
    let mut input = object(fields.clone());
    input.insert("book_id".to_owned(), json!("b1"));
    command
        .host
        .submit("probe.ints.AddBook", input)
        .unwrap_or_else(|error| panic!("AddBook runs: {error}"));
    let from_command = listed(&mut command.host, BOOKS).remove(0);
    (from_world, from_command)
}

fn fields(pages: Value, weight: Value) -> Value {
    json!({"pages": pages, "weight": weight, "spare": null, "counts": [1, 2],
           "size": {"width": 3}, "printed": "2026-01-02T03:04:05Z"})
}

/// Parity probes: a world row and a command row of the same spelling answer alike. A command's
/// Integer input spelled `1.0` is still held as written (story:integer-identity-any-spelling), so
/// that spelling is not probed here.
#[test]
fn a_world_row_answers_what_a_command_row_of_the_same_spelling_answers() {
    for (pages, weight) in [
        (json!(1), json!(1)),
        (json!(9_007_199_254_740_993_i64), json!(1.5)),
    ] {
        let given = fields(pages.clone(), weight.clone());
        let (world, command) = world_and_command(&given);
        assert_eq!(world, command, "pages {pages}, weight {weight}");
    }
}

/// ESS reads `1` and `1.0` of an `Integer` identity as one value, and the base refused the second
/// row as `already a row of this world`. The world reader now holds both spellings, and the host
/// keys an instance by its identity's JSON text (`spindle-kernel/src/store.rs:565`), so one
/// instance is loaded twice.
#[test]
fn a_world_refuses_one_integer_identity_written_twice() {
    let mut opened = host();
    let loaded = reset(
        &mut opened.host,
        &json!({"rows": [shelf(json!(1), "top"), shelf(json!(1.0), "bottom")]}),
    );
    let shelves = listed(&mut opened.host, SHELVES);
    assert!(
        loaded.is_err(),
        "one shelf written as 1 and as 1.0 loads as two rows: {shelves:?}"
    );
}

/// A book in a world names its shelf `1`; the shelf in the same world is written `1.0`. ESS
/// reads them as one value, and the base resolved the reference (both held as `1.0`).
#[test]
fn a_world_reference_names_its_row_whatever_the_integer_spelling() {
    let mut opened = host();
    let mut fields = fields(json!(1), json!(1));
    fields["shelf_id"] = json!(1);
    reset(
        &mut opened.host,
        &json!({"rows": [shelf(json!(1.0), "top"), book("b1", &fields)]}),
    )
    .unwrap_or_else(|why| panic!("the world loads: {why}"));
    let books = listed(&mut opened.host, BOOKS);
    assert_eq!(
        books[0].get("shelf_label"),
        Some(&json!("top")),
        "{books:?}"
    );
}

/// A world row whose `Integer` identity is written `1.0`, then a command naming it `1`, the
/// spelling a command's caller writes an integer in. ESS reads the two as one instance.
#[test]
fn a_command_names_a_world_row_whatever_the_integer_spelling() {
    let mut opened = host();
    reset(
        &mut opened.host,
        &json!({"rows": [shelf(json!(1.0), "top")]}),
    )
    .unwrap_or_else(|why| panic!("the world loads: {why}"));
    let renamed = opened.host.submit(
        "probe.ints.RenameShelf",
        object(json!({"shelf_id": 1, "label": "renamed"})),
    );
    let shelves = listed(&mut opened.host, SHELVES);
    assert!(renamed.is_ok(), "{renamed:?}; {shelves:?}");
    assert_eq!(
        shelves,
        [object(json!({"shelf_id": 1, "label": "renamed"}))]
    );
}

/// The story's Outcome: a world row answers its `Integer` fields as integers. Written `1.0`, a
/// value ESS admits as the integer 1, it answers `1.0`.
#[test]
fn a_world_row_answers_an_integer_written_1_0_as_an_integer() {
    let mut opened = host();
    reset(
        &mut opened.host,
        &json!({"rows": [book("b1", &fields(json!(1.0), json!(1)))]}),
    )
    .unwrap_or_else(|why| panic!("the world loads: {why}"));
    let books = listed(&mut opened.host, BOOKS);
    assert_eq!(
        serde_json::to_string(&books[0]["pages"]).expect("writes"),
        "1",
        "{books:?}"
    );
}

/// A filter (`pages == 1`) and an `order_by` over world rows written `1` and `1.0`: ESS reads
/// both as 1, so both rows are kept.
#[test]
fn a_filter_keeps_a_world_integer_in_either_spelling() {
    let mut opened = host();
    reset(
        &mut opened.host,
        &json!({"rows": [book("b1", &fields(json!(1), json!(1))),
                         book("b2", &fields(json!(1.0), json!(1)))]}),
    )
    .unwrap_or_else(|why| panic!("the world loads: {why}"));
    let kept = listed(&mut opened.host, "probe.ints.OnePage");
    let ids: Vec<&Value> = kept.iter().map(|row| &row["book_id"]).collect();
    assert_eq!(ids, [&json!("b1"), &json!("b2")], "{kept:?}");
}
