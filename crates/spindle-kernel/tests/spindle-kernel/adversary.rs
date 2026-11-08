//! Adversary cases: each drives the kernel against ESS 0.85.0's reference interpreter
//! (`crates/verify/ess-conformance/src/interpret`) on the fixture `tests/fixtures/adversary`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Row, SqliteStore, Store, StoreError,
    execute,
};
use spindle_spec::EssIr;

const DOC: &str = "probe.docs.Doc";
const ID: &str = "123e4567-e89b-12d3-a456-426614174000";

fn docs() -> EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary");
    spindle_spec::load(&root)
        .expect("the adversary fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("an input is an object, not {other}"),
    }
}

fn attempt(
    ir: &EssIr,
    store: &mut dyn Store,
    command: &str,
    request: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &input(request))
}

fn run(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value) -> Executed {
    attempt(ir, store, command, request)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn create(ir: &EssIr, store: &mut dyn Store, capacity: i64) {
    let created = run(
        ir,
        store,
        "probe.docs.CreateDoc",
        json!({"doc_id": ID, "capacity": capacity}),
    );
    assert_eq!(created.outcome, "created");
}

/// ESS `interpret/execute.rs` `take` ends with `at_rest`, which refuses a row that breaks its
/// entity's invariant (`Undetermined::BrokenInvariant`): the step is not taken and nothing rests.
#[test]
fn a_creation_that_breaks_an_entity_invariant_changes_nothing() {
    let ir = docs();
    let mut store = MemoryStore::default();
    let result = attempt(
        &ir,
        &mut store,
        "probe.docs.CreateDoc",
        json!({"doc_id": ID, "capacity": 0}),
    );
    assert!(
        result.is_err(),
        "`capacity > 0` is the entity's invariant; ESS refuses the step, the kernel answered {result:?}"
    );
    assert_eq!(store.load(DOC, &json!(ID)).expect("the store reads"), None);
    assert!(store.events().is_empty(), "{:?}", store.events());
}

/// ESS `interpret/execute/determined.rs` `increment` checks the sum with
/// `input::validate_typed_value` against the target type, whose newtype invariant is
/// `value >= 0`: a sum of -1 is refused and the row keeps its level.
#[test]
fn an_increment_below_a_newtype_invariant_changes_nothing() {
    let ir = docs();
    let mut store = MemoryStore::default();
    create(&ir, &mut store, 5);
    let lowered = run(&ir, &mut store, "probe.docs.Lower", json!({"doc_id": ID}));
    assert_eq!(lowered.outcome, "lowered");
    let before = store.load(DOC, &json!(ID)).unwrap().unwrap();
    assert_eq!(before.row.fields.get("level"), Some(&json!(0)));

    let result = attempt(&ir, &mut store, "probe.docs.Lower", json!({"doc_id": ID}));
    assert!(
        result.is_err(),
        "`probe.docs.Level` admits no value below 0; the kernel answered {result:?}"
    );
    assert_eq!(store.load(DOC, &json!(ID)).unwrap().unwrap(), before);
}

/// ESS `Primitive::admits` (`ess-domain/src/types.rs`) admits a `Uuid` only where
/// `is_canonical_uuid` holds; the conformance interpreter refuses any other input value
/// (`input::project_value` through `primitive_value`).
#[test]
fn a_non_canonical_uuid_is_invalid_input() {
    let ir = docs();
    let mut store = MemoryStore::default();
    let result = attempt(
        &ir,
        &mut store,
        "probe.docs.CreateDoc",
        json!({"doc_id": "not-a-uuid", "capacity": 3}),
    );
    assert!(
        matches!(result, Err(ExecuteError::InvalidInput(_))),
        "`not-a-uuid` is no value of `Uuid`; the kernel answered {result:?}"
    );
    assert_eq!(
        store
            .load(DOC, &json!("not-a-uuid"))
            .expect("the store reads"),
        None
    );
}

/// ESS `Primitive::admits` admits an `Integer` wherever the number `is_integral`, so `3.0` is the
/// integer 3, as the conformance interpreter reads a candidate.
#[test]
fn an_integral_number_spelt_with_a_fraction_is_an_integer_input() {
    let ir = docs();
    let mut store = MemoryStore::default();
    let result = attempt(
        &ir,
        &mut store,
        "probe.docs.CreateDoc",
        json!({"doc_id": ID, "capacity": 3.0}),
    );
    let created = result.unwrap_or_else(|error| panic!("3.0 is an integral Integer: {error}"));
    assert_eq!(created.outcome, "created");
}

/// ESS `interpret/execute.rs` `unknown_instance` falls back to `wrong_state(.., None)`, and
/// `error_value` reads `{subject: state}` as absent where no row is held ("The unknown-instance arm
/// of a `wrong_state:` refusal holds no row, and its error carries no field read from one",
/// beyond10x/ess#458). The answer is the `conflict` error without `current`.
#[test]
fn the_wrong_state_error_for_an_unknown_document_carries_no_current_state() {
    let ir = docs();
    let mut store = MemoryStore::default();
    let result = attempt(
        &ir,
        &mut store,
        "probe.docs.PublishDoc",
        json!({"doc_id": ID}),
    );
    let answered = result.unwrap_or_else(|error| {
        panic!("ESS answers the `conflict` outcome for an unknown document, not {error}")
    });
    assert_eq!(answered.outcome, "conflict");
    let error = answered.error.expect("the conflict error");
    assert_eq!(error.name, "probe.docs.StateConflict");
    assert_eq!(Value::Object(error.fields), json!({"doc_id": ID}));
    assert!(answered.events.is_empty());
    assert_eq!(store.load(DOC, &json!(ID)).unwrap(), None);
}

/// ESS `interpret/execute.rs` `select_phases`: a `wrong_state:` branch is a marker, read as
/// `Read::Passes`; the wrong-state answer comes only from `act` returning `NotFromHere` for the
/// branch selected. A published document with `flag == true` selects `noted`, an `updates:` that
/// does not move, so the note is written.
#[test]
fn an_accepting_update_is_taken_from_a_state_no_move_starts_from() {
    let ir = docs();
    let mut store = MemoryStore::default();
    create(&ir, &mut store, 3);
    let published = run(
        &ir,
        &mut store,
        "probe.docs.Annotate",
        json!({"doc_id": ID, "flag": false, "note": "unused"}),
    );
    assert_eq!(published.outcome, "published");

    let noted = run(
        &ir,
        &mut store,
        "probe.docs.Annotate",
        json!({"doc_id": ID, "flag": true, "note": "late"}),
    );
    assert_eq!(noted.outcome, "noted", "{noted:?}");
    assert_eq!(noted.error, None);
    let stored = store.load(DOC, &json!(ID)).unwrap().unwrap();
    assert_eq!(stored.row.state, "Published");
    assert_eq!(stored.row.fields.get("note"), Some(&json!("late")));
}

/// Two handles on one SQLite file that read the same version: the second commit is a conflict and
/// leaves the first writer's row in place.
#[test]
fn two_writers_of_one_sqlite_file_cannot_both_commit() {
    let ir = {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess");
        spindle_spec::load(&root).expect("ess/ compiles").ir
    };
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let path = directory.path().join("state.sqlite");
    let mut first = SqliteStore::open(&path).expect("the first handle opens");
    let mut second = SqliteStore::open(&path).expect("the second handle opens");
    let offered = run(
        &ir,
        &mut first,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop", "requires": []}),
    );
    assert_eq!(offered.outcome, "offered");

    let entity = "spindle.deployment.Deployment";
    let a = first.load(entity, &json!("d1")).unwrap().unwrap();
    let b = second.load(entity, &json!("d1")).unwrap().unwrap();
    assert_eq!(a.version, b.version);
    let change = |state: &str, version: u64| Change {
        entity: entity.to_owned(),
        identity: json!("d1"),
        expected: Some(version),
        row: Row {
            state: state.to_owned(),
            fields: BTreeMap::new(),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    };
    first
        .commit(&change("Admitted", a.version))
        .expect("the first writer commits");
    let refused = second.commit(&change("Refused", b.version));
    assert!(
        matches!(refused, Err(StoreError::Conflict { .. })),
        "{refused:?}"
    );
    assert_eq!(
        second
            .load(entity, &json!("d1"))
            .unwrap()
            .unwrap()
            .row
            .state,
        "Admitted"
    );
}
