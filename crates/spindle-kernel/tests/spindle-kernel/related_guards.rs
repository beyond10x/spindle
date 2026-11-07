//! `when_related:` over one row (ess/18, ess/22): the row of another entity whose identity an
//! input field, or a stored field of the addressed subject, names, read as the store held it
//! before selection, as ESS's interpreter reads it (`interpret::execute::related`).
//!
//! The fixture `tests/fixtures/warehouse` declares shelves that open and close, and items.
//! `StockItem` names its shelf by an `Optional` input, `PickItem` by a required one; each refuses
//! with `no-shelf` on `exists: false` and with `shelf-closed` where the shelf is `Closed`.
//! `ReturnItem` reads the shelf the item stores in `home` and refuses with `home-gone` and
//! `home-closed` the same way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Row, Store, execute,
};

const SHELF: &str = "depot.warehouse.Shelf";
const ITEM: &str = "depot.warehouse.Item";
const OPEN: &str = "depot.warehouse.OpenShelf";
const CLOSE: &str = "depot.warehouse.CloseShelf";
const STOCK: &str = "depot.warehouse.StockItem";
const PICK: &str = "depot.warehouse.PickItem";
const RETURN: &str = "depot.warehouse.ReturnItem";

const NORTH: &str = "1a2b3c4d-0000-4000-8000-000000000001";
const SOUTH: &str = "1a2b3c4d-0000-4000-8000-000000000002";
const NOWHERE: &str = "1a2b3c4d-0000-4000-8000-0000000000ff";
const LAMP: &str = "7e8f9a0b-0000-4000-8000-000000000001";
const VASE: &str = "7e8f9a0b-0000-4000-8000-000000000002";
const DESK: &str = "7e8f9a0b-0000-4000-8000-000000000003";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/warehouse")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the warehouse fixture compiles")
        .ir
}

pub(crate) fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input))
}

pub(crate) fn run(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    command: &str,
    input: Value,
) -> Executed {
    send(ir, store, command, input.clone())
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn error(executed: &Executed) -> Option<&str> {
    executed.error.as_ref().map(|error| error.name.as_str())
}

fn open(ir: &spindle_spec::EssIr, store: &mut MemoryStore, shelf: &str) {
    let opened = run(ir, store, OPEN, json!({"shelf_id": shelf, "label": shelf}));
    assert_eq!(opened.outcome, "opened");
}

fn close(ir: &spindle_spec::EssIr, store: &mut MemoryStore, shelf: &str) {
    let closed = run(ir, store, CLOSE, json!({"shelf_id": shelf}));
    assert_eq!(closed.outcome, "closed");
}

fn row(store: &mut MemoryStore, entity: &str, identity: &str) -> Option<Row> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .map(|stored| stored.row)
}

#[test]
fn a_missing_related_row_takes_the_exists_false_branch() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH);
    let stocked = run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(stocked.outcome, "stocked");
    let before = row(&mut store, ITEM, LAMP);
    let events = store.events().len();

    let refused = run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": LAMP, "shelf_id": NOWHERE}),
    );
    assert_eq!(refused.outcome, "no-shelf");
    assert_eq!(error(&refused), Some("depot.warehouse.NoShelf"));
    assert!(refused.events.is_empty());
    assert_eq!(
        row(&mut store, ITEM, LAMP),
        before,
        "the item is not picked"
    );

    let refused = run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": VASE, "shelf_id": NOWHERE}),
    );
    assert_eq!(refused.outcome, "no-shelf");
    assert_eq!(error(&refused), Some("depot.warehouse.NoShelf"));
    assert_eq!(row(&mut store, ITEM, VASE), None, "no item is created");
    assert_eq!(row(&mut store, SHELF, NOWHERE), None, "no shelf is created");
    assert_eq!(store.events().len(), events, "nothing is written");
}

#[test]
fn a_test_on_the_related_row_selects_its_branch() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH);
    open(&ir, &mut store, SOUTH);
    run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": LAMP, "shelf_id": NORTH}),
    );
    close(&ir, &mut store, SOUTH);
    let events = store.events().len();

    let refused = run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": LAMP, "shelf_id": SOUTH}),
    );
    assert_eq!(refused.outcome, "shelf-closed");
    assert_eq!(error(&refused), Some("depot.warehouse.ShelfShut"));
    assert!(refused.events.is_empty());
    assert_eq!(store.events().len(), events, "a refusal writes nothing");
    assert_eq!(
        row(&mut store, ITEM, LAMP).map(|row| row.state),
        Some("Stocked".to_owned())
    );

    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(picked.outcome, "picked");
    assert_eq!(error(&picked), None);
    assert_eq!(picked.events.len(), 1);
    assert_eq!(picked.events[0].name, "depot.warehouse.ItemPicked");
    assert_eq!(picked.events[0].payload["shelf_id"], json!(NORTH));
    assert_eq!(
        row(&mut store, ITEM, LAMP).map(|row| row.state),
        Some("Picked".to_owned())
    );

    let refused = run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": VASE, "shelf_id": SOUTH}),
    );
    assert_eq!(
        refused.outcome, "shelf-closed",
        "the optional reference, present, is read"
    );
    assert_eq!(row(&mut store, ITEM, VASE), None);
}

#[test]
fn an_absent_optional_reference_selects_no_related_branch() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH);

    let stocked = run(&ir, &mut store, STOCK, json!({"item_id": LAMP}));
    assert_eq!(stocked.outcome, "stocked", "an omitted shelf reads no row");
    let stocked = run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": VASE, "shelf_id": null}),
    );
    assert_eq!(stocked.outcome, "stocked", "a null shelf reads no row");
    for item in [LAMP, VASE] {
        let held = row(&mut store, ITEM, item).expect("the item is stocked");
        assert_eq!(held.state, "Stocked");
        assert_eq!(held.fields.get("home").filter(|home| !home.is_null()), None);
    }

    let picked = run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": LAMP, "shelf_id": NORTH}),
    );
    assert_eq!(picked.outcome, "picked");
    let returned = run(&ir, &mut store, RETURN, json!({"item_id": LAMP}));
    assert_eq!(
        returned.outcome, "returned",
        "an item with no home reads no shelf and selects neither `home-gone` nor `home-closed`"
    );
}

#[test]
fn a_reference_stored_on_the_subject_is_followed() {
    let ir = ir();
    let mut store = MemoryStore::default();
    open(&ir, &mut store, NORTH);
    open(&ir, &mut store, SOUTH);
    run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": LAMP, "shelf_id": NORTH}),
    );
    run(
        &ir,
        &mut store,
        STOCK,
        json!({"item_id": VASE, "shelf_id": SOUTH}),
    );
    run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": LAMP, "shelf_id": SOUTH}),
    );
    run(
        &ir,
        &mut store,
        PICK,
        json!({"item_id": VASE, "shelf_id": SOUTH}),
    );
    close(&ir, &mut store, NORTH);

    let refused = run(&ir, &mut store, RETURN, json!({"item_id": LAMP}));
    assert_eq!(
        refused.outcome, "home-closed",
        "the lamp's home is the closed north shelf, not the open south one it was picked from"
    );
    assert_eq!(error(&refused), Some("depot.warehouse.ShelfShut"));
    assert_eq!(
        row(&mut store, ITEM, LAMP).map(|row| row.state),
        Some("Picked".to_owned())
    );

    let returned = run(&ir, &mut store, RETURN, json!({"item_id": VASE}));
    assert_eq!(
        returned.outcome, "returned",
        "the vase's home, south, is open"
    );

    let wrong = run(&ir, &mut store, RETURN, json!({"item_id": VASE}));
    assert_eq!(
        wrong.outcome, "wrong-state",
        "a stocked item answers its held state before its home is read"
    );

    store
        .commit(&Change {
            entity: ITEM.to_owned(),
            identity: json!(DESK),
            expected: None,
            row: Row {
                state: "Picked".to_owned(),
                fields: BTreeMap::from([("home".to_owned(), json!(NOWHERE))]),
            },
            events: Vec::new(),
        })
        .expect("the store takes the seeded desk");
    let gone = run(&ir, &mut store, RETURN, json!({"item_id": DESK}));
    assert_eq!(
        gone.outcome, "home-gone",
        "a stored identity no shelf carries takes the `exists: false` branch"
    );
    assert_eq!(error(&gone), Some("depot.warehouse.NoShelf"));
}

/// What one step answered: the outcome, the declared error, and the events by name and payload.
pub(crate) type Answer = (String, Option<String>, Vec<(String, Map<String, Value>)>);

/// The rows of each view after the last step, `null` members left out.
pub(crate) type Rows = Vec<Vec<Map<String, Value>>>;

const VIEWS: [(&str, &str, &str); 2] = [
    ("depot.warehouse.ShelfCard", SHELF, "shelf_id"),
    ("depot.warehouse.ItemCard", ITEM, "item_id"),
];

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

fn present(row: Map<String, Value>) -> Map<String, Value> {
    row.into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

/// What ESS's interpreter answers for each step, and the rows of each of `views` (view, entity,
/// identity field) after the last.
pub(crate) fn interpreted(
    ir: &spindle_spec::EssIr,
    views: &[(&str, &str, &str)],
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Rows) {
    interpreted_on(Interpreted::for_model(ir.clone()), views, steps)
}

/// [`interpreted`], on `target`: the interpreter as the caller configured it, such as with a
/// command clock.
pub(crate) fn interpreted_on(
    target: Interpreted,
    views: &[(&str, &str, &str)],
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Rows) {
    let context = ScenarioContext::new(
        "depot.warehouse/authored/related"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("related").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("the interpreter runs {command}: {error}"));
        answers.push((
            executed
                .outcome
                .map(|outcome| outcome.outcome.as_str().to_owned())
                .unwrap_or_default(),
            executed.error.map(|error| error.error.to_string()),
            executed
                .direct_events
                .iter()
                .map(|event| {
                    (
                        event.event.to_string(),
                        event
                            .payload
                            .iter()
                            .map(|(field, node)| (field.clone(), json_of(node)))
                            .collect(),
                    )
                })
                .collect(),
        ));
    }
    let rows = views
        .iter()
        .map(|(view, _, _)| {
            target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: ess_primitives::consistency::QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
                })
                .expect("the interpreter reads the view")
                .rows
                .into_iter()
                .map(|row| {
                    present(
                        row.iter()
                            .map(|(field, node)| (field.clone(), json_of(node)))
                            .collect(),
                    )
                })
                .collect()
        })
        .collect();
    (answers, rows)
}

/// What the kernel answers for each step, and each stored row of every entity of `views` as its
/// view projects it.
pub(crate) fn spindled(
    ir: &spindle_spec::EssIr,
    views: &[(&str, &str, &str)],
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Rows) {
    spindled_with(views, steps, &mut |store, command, input| {
        run(ir, store, command, input)
    })
}

/// [`spindled`], each step executed by `execute`, such as on a fixed clock.
pub(crate) fn spindled_with(
    views: &[(&str, &str, &str)],
    steps: &[(&str, Value)],
    execute: &mut dyn FnMut(&mut MemoryStore, &str, Value) -> Executed,
) -> (Vec<Answer>, Rows) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for (command, input) in steps {
        let executed = execute(&mut store, command, input.clone());
        answers.push((
            executed.outcome,
            executed.error.map(|error| error.name),
            executed
                .events
                .into_iter()
                .map(|event| (event.name, event.payload))
                .collect(),
        ));
    }
    let rows = views
        .iter()
        .map(|(_, entity, identity_field)| {
            store
                .scan(entity)
                .expect("the store scans")
                .into_iter()
                .map(|(identity, stored)| {
                    let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
                    row.insert((*identity_field).to_owned(), identity);
                    row.insert("state".to_owned(), json!(stored.row.state));
                    present(row)
                })
                .collect()
        })
        .collect();
    (answers, rows)
}

/// The steps the agreement is read over: every branch of `StockItem`, `PickItem` and
/// `ReturnItem` but the seeded `home-gone`, and a picked item asked to be picked again from a
/// closed shelf, which answers its held state before the shelf's.
fn steps() -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"shelf_id": NORTH, "label": "north"})),
        (OPEN, json!({"shelf_id": SOUTH, "label": "south"})),
        (STOCK, json!({"item_id": LAMP, "shelf_id": NORTH})),
        (STOCK, json!({"item_id": VASE, "shelf_id": SOUTH})),
        (STOCK, json!({"item_id": DESK})),
        (STOCK, json!({"item_id": DESK, "shelf_id": NOWHERE})),
        (PICK, json!({"item_id": LAMP, "shelf_id": NOWHERE})),
        (PICK, json!({"item_id": LAMP, "shelf_id": SOUTH})),
        (PICK, json!({"item_id": VASE, "shelf_id": SOUTH})),
        (PICK, json!({"item_id": DESK, "shelf_id": NORTH})),
        (CLOSE, json!({"shelf_id": NORTH})),
        (PICK, json!({"item_id": LAMP, "shelf_id": NORTH})),
        (
            STOCK,
            json!({"item_id": "7e8f9a0b-0000-4000-8000-000000000004", "shelf_id": NORTH}),
        ),
        (RETURN, json!({"item_id": LAMP})),
        (RETURN, json!({"item_id": VASE})),
        (RETURN, json!({"item_id": DESK})),
        (RETURN, json!({"item_id": VASE})),
        (
            PICK,
            json!({"item_id": "7e8f9a0b-0000-4000-8000-0000000000aa", "shelf_id": SOUTH}),
        ),
    ]
}

/// The `unknown_instance:` branch of `PickItem` and `ReturnItem`. Before ess/27 a command may not
/// declare one beside `when_related:`.
const NO_ITEM: &str = "      - name: no-item
        unknown_instance: true
        error: depot.warehouse.NoItem
        summary: No item carries the id.
";

/// The fixture before the one rule (ess/26), where an input's missing row answers at step 1,
/// before the input refusals, and without its `unknown_instance:` branches, in a directory of its
/// own.
fn before_the_one_rule() -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    for file in ["system.yaml", "domains/warehouse.yaml"] {
        let text = std::fs::read_to_string(fixture().join(file)).expect("the fixture reads");
        assert!(
            file == "system.yaml" || text.matches(NO_ITEM).count() == 2,
            "the fixture declares `no-item` twice"
        );
        let text = text
            .replacen("format: ess/34", "format: ess/26", 1)
            .replace(NO_ITEM, "");
        std::fs::write(dir.path().join(file), text).expect("the copy writes");
    }
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the warehouse fixture compiles at ess/26: {error}"))
        .ir;
    (dir, ir)
}

#[test]
fn related_row_guards_agree_with_the_interpreter() {
    let steps = steps();
    let expected = [
        "opened",
        "opened",
        "stocked",
        "stocked",
        "stocked",
        "no-shelf",
        "no-shelf",
        "picked",
        "picked",
        "picked",
        "closed",
        "wrong-state",
        "shelf-closed",
        "home-closed",
        "returned",
        "returned",
        "wrong-state",
        "no-item",
    ];
    let (_dir, older) = before_the_one_rule();
    let known = steps.len() - 1;
    for (format, ir, steps, expected) in [
        ("ess/34", ir(), &steps[..], &expected[..]),
        ("ess/26", older, &steps[..known], &expected[..known]),
    ] {
        let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, steps);
        let outcomes: Vec<&str> = ess_answers
            .iter()
            .map(|(outcome, _, _)| outcome.as_str())
            .collect();
        assert_eq!(
            outcomes, expected,
            "at {format} the steps reach every branch the related row decides"
        );
        let (answers, rows) = spindled(&ir, &VIEWS, steps);
        assert_eq!(
            answers, ess_answers,
            "at {format} outcomes, errors and events agree"
        );
        assert_eq!(
            rows, ess_rows,
            "at {format} the stored shelves and items agree with ESS's view rows"
        );
    }
}
