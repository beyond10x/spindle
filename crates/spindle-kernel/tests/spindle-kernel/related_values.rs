//! Values read from a related row (ess/16, ess/28): `{related: {via: input.<field>, field: …}}`
//! in `sets:` or `payload:` reads the field, or with `field: state` the held state, of the row an
//! input field names, as the store held it before the outcome, as ESS's interpreter reads it
//! (`interpret::execute::determined::Reads::related`).
//!
//! The fixture `tests/fixtures/warehouse-picks` is the warehouse of the related-row guards with
//! aisles and picks: a shelf stands in an aisle, `MoveShelf` moves it and keeps the aisle it left,
//! `StockItem` copies the aisle of the shelf an `Optional` input names, and a successful
//! `PickItem` creates a pick that stores the shelf's aisle and state and publishes both.

use std::path::Path;

use serde_json::{Value, json};
use spindle_kernel::{MemoryStore, Row, Store};

use super::related_guards::{interpreted, run, spindled};

const SHELF: &str = "depot.warehouse.Shelf";
const ITEM: &str = "depot.warehouse.Item";
const PICK_ROW: &str = "depot.warehouse.Pick";
const OPEN: &str = "depot.warehouse.OpenShelf";
const MOVE: &str = "depot.warehouse.MoveShelf";
const CLOSE: &str = "depot.warehouse.CloseShelf";
const STOCK: &str = "depot.warehouse.StockItem";
const PICK: &str = "depot.warehouse.PickItem";

const NORTH: &str = "1a2b3c4d-0000-4000-8000-000000000001";
const SOUTH: &str = "1a2b3c4d-0000-4000-8000-000000000002";
const NOWHERE: &str = "1a2b3c4d-0000-4000-8000-0000000000ff";
const LAMP: &str = "7e8f9a0b-0000-4000-8000-000000000001";
const VASE: &str = "7e8f9a0b-0000-4000-8000-000000000002";
const FIRST: &str = "5c6d7e8f-0000-4000-8000-000000000001";
const SECOND: &str = "5c6d7e8f-0000-4000-8000-000000000002";
const THIRD: &str = "5c6d7e8f-0000-4000-8000-000000000003";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/warehouse-picks"),
    )
    .expect("the warehouse-picks fixture compiles")
    .ir
}

fn open(ir: &spindle_spec::EssIr, store: &mut MemoryStore, shelf: &str, aisle: &str) {
    let opened = run(
        ir,
        store,
        OPEN,
        json!({"shelf_id": shelf, "label": shelf, "aisle": aisle}),
    );
    assert_eq!(opened.outcome, "opened");
}

fn row(store: &mut MemoryStore, entity: &str, identity: &str) -> Row {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .unwrap_or_else(|| panic!("a `{entity}` {identity} is held"))
        .row
}

#[test]
fn a_created_row_copies_a_field_of_the_related_row() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH, "A7");
    open(&ir, &mut store, SOUTH, "B2");

    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"pick_id": FIRST, "item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(picked.outcome, "picked");
    assert_eq!(
        row(&mut store, PICK_ROW, FIRST).fields["aisle"],
        json!("A7")
    );
    assert_eq!(picked.events.len(), 1);
    assert_eq!(picked.events[0].name, "depot.warehouse.ItemPicked");
    assert_eq!(picked.events[0].payload["aisle"], json!("A7"));

    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"pick_id": SECOND, "item_id": LAMP, "shelf_id": SOUTH}),
    );
    assert_eq!(picked.events[0].payload["aisle"], json!("B2"));
    assert_eq!(
        row(&mut store, PICK_ROW, SECOND).fields["aisle"],
        json!("B2"),
        "each pick reads the shelf its own input names"
    );

    let stocked = run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": LAMP, "shelf_id": SOUTH}),
    );
    assert_eq!(stocked.outcome, "stocked");
    assert_eq!(row(&mut store, ITEM, LAMP).fields["aisle"], json!("B2"));
    let stocked = run(&ir, &mut store, STOCK, json!({"item_id": VASE}));
    assert_eq!(stocked.outcome, "stocked");
    assert_eq!(
        row(&mut store, ITEM, VASE)
            .fields
            .get("aisle")
            .filter(|aisle| !aisle.is_null()),
        None,
        "an absent optional reference reads no row and copies nothing"
    );

    let refused = run(
        &ir,
        &mut store,
        PICK,
        json!({"pick_id": THIRD, "item_id": LAMP, "shelf_id": NOWHERE}),
    );
    assert_eq!(refused.outcome, "no-shelf");
    assert!(
        store
            .load(PICK_ROW, &json!(THIRD))
            .expect("reads")
            .is_none(),
        "a missing shelf creates no pick"
    );
}

#[test]
fn the_related_row_is_read_before_the_outcome() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH, "A7");

    let moved = run(
        &ir,
        &mut store,
        MOVE,
        json!({"shelf_id": NORTH, "aisle": "C3"}),
    );
    assert_eq!(moved.outcome, "moved");
    let shelf = row(&mut store, SHELF, NORTH);
    assert_eq!(shelf.fields["aisle"], json!("C3"), "the shelf moves");
    assert_eq!(
        shelf.fields["previous_aisle"],
        json!("A7"),
        "the outcome changes the shelf it reads, and stores the aisle the shelf held before"
    );
    assert_eq!(moved.events[0].payload["from"], json!("A7"));
    assert_eq!(moved.events[0].payload["to"], json!("C3"));

    let moved = run(
        &ir,
        &mut store,
        MOVE,
        json!({"shelf_id": NORTH, "aisle": "D4"}),
    );
    assert_eq!(moved.events[0].payload["from"], json!("C3"));
    assert_eq!(
        row(&mut store, SHELF, NORTH).fields["previous_aisle"],
        json!("C3")
    );
}

#[test]
fn a_related_state_value_reads_the_held_state() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH, "A7");

    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"pick_id": FIRST, "item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(picked.events[0].payload["shelf_state"], json!("Open"));
    assert_eq!(
        row(&mut store, PICK_ROW, FIRST).fields["shelf_state"],
        json!("Open")
    );

    let closed = run(&ir, &mut store, CLOSE, json!({"shelf_id": NORTH}));
    assert_eq!(closed.outcome, "closed");
    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"pick_id": SECOND, "item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(picked.outcome, "picked");
    assert_eq!(picked.events[0].payload["shelf_state"], json!("Closed"));
    assert_eq!(
        row(&mut store, PICK_ROW, SECOND).fields["shelf_state"],
        json!("Closed")
    );
}

const VIEWS: [(&str, &str, &str); 3] = [
    ("depot.warehouse.ShelfCard", SHELF, "shelf_id"),
    ("depot.warehouse.ItemCard", ITEM, "item_id"),
    ("depot.warehouse.PickCard", PICK_ROW, "pick_id"),
];

#[test]
fn related_values_agree_with_the_interpreter() {
    let ir = ir();
    let steps: Vec<(&str, Value)> = vec![
        (
            OPEN,
            json!({"shelf_id": NORTH, "label": "north", "aisle": "A7"}),
        ),
        (
            OPEN,
            json!({"shelf_id": SOUTH, "label": "south", "aisle": "B2"}),
        ),
        (STOCK, json!({"item_id": LAMP, "shelf_id": NORTH})),
        (STOCK, json!({"item_id": VASE})),
        (STOCK, json!({"item_id": VASE, "shelf_id": NOWHERE})),
        (
            PICK,
            json!({"pick_id": FIRST, "item_id": LAMP, "shelf_id": NORTH}),
        ),
        (MOVE, json!({"shelf_id": NORTH, "aisle": "C3"})),
        (CLOSE, json!({"shelf_id": SOUTH})),
        (
            PICK,
            json!({"pick_id": SECOND, "item_id": VASE, "shelf_id": SOUTH}),
        ),
        (
            PICK,
            json!({"pick_id": THIRD, "item_id": LAMP, "shelf_id": NOWHERE}),
        ),
        (
            PICK,
            json!({"pick_id": THIRD, "item_id": LAMP, "shelf_id": NORTH}),
        ),
    ];
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "opened", "opened", "stocked", "stocked", "no-shelf", "picked", "moved", "closed",
            "picked", "no-shelf", "picked"
        ],
        "the steps copy a field and a state, through a present and an absent reference"
    );
    let (answers, rows) = spindled(&ir, &VIEWS, &steps);
    assert_eq!(
        answers, ess_answers,
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        rows, ess_rows,
        "the stored shelves, items and picks agree with ESS's view rows"
    );
}
