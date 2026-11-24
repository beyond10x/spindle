//! A stored field a row holds as `null`, copied into an event (`{subject: <field>}`), is carried
//! as `null` on every store, as ESS's interpreter carries it (`determined::subject`); a field the
//! row does not hold is left out, unless the event field declares `null_when_absent`. Over the
//! `relay` fixture of `spindle-conformance`, whose scenario test establishes such a row.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, MemoryStore, NoExternals, Row, SqliteStore, Store, execute};

use super::related_guards::object;

const DESK: &str = "relay.desk.Desk";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../spindle-conformance/tests/fixtures/held-null"),
    )
    .expect("the relay fixture compiles")
    .ir
}

/// Writes desk `identity` holding `fields` as they are given, `null` included.
fn rest(store: &mut dyn Store, identity: &str, fields: Value) {
    let Value::Object(fields) = fields else {
        panic!("fields are an object")
    };
    store
        .commit(&Change {
            entity: DESK.to_owned(),
            identity: json!(identity),
            expected: None,
            row: Row {
                state: "Open".to_owned(),
                fields: fields.into_iter().collect::<BTreeMap<_, _>>(),
            },
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("the store takes the desk");
}

/// The payload `HandDesk` publishes for `desk`.
fn handed(ir: &spindle_spec::EssIr, store: &mut dyn Store, desk: &str) -> Map<String, Value> {
    let executed = execute(
        ir,
        store,
        &mut NoExternals,
        "relay.desk.HandDesk",
        &object(json!({"desk_id": desk, "keeper_id": "keeper-b"})),
    )
    .expect("the desk is handed");
    assert_eq!(executed.outcome, "handed");
    executed.events[0].payload.clone()
}

fn check(ir: &spindle_spec::EssIr, store: &mut dyn Store) {
    let held = handed(ir, store, "desk-1");
    assert_eq!(held.get("previous_keeper_id"), Some(&Value::Null));
    assert_eq!(held.get("noted_keeper_id"), Some(&Value::Null));
    let never = handed(ir, store, "desk-2");
    assert_eq!(never.get("previous_keeper_id"), None);
    assert_eq!(never.get("noted_keeper_id"), Some(&Value::Null));
}

#[test]
fn a_held_null_is_carried_as_null_on_memory_and_sqlite() {
    let ir = ir();
    let mut memory = MemoryStore::default();
    rest(&mut memory, "desk-1", json!({"keeper_id": null}));
    rest(&mut memory, "desk-2", json!({}));
    check(&ir, &mut memory);

    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("relay.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        rest(&mut store, "desk-1", json!({"keeper_id": null}));
        rest(&mut store, "desk-2", json!({}));
    }
    let mut store = SqliteStore::open(&path).expect("SQLite reopens");
    assert_eq!(
        store
            .load(DESK, &json!("desk-1"))
            .expect("reads")
            .expect("held")
            .row
            .fields
            .get("keeper_id"),
        Some(&Value::Null),
        "the held null survives a restart"
    );
    check(&ir, &mut store);
}
