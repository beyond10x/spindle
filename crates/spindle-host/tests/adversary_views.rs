//! Adversarial case for list-view ordering (`crates/spindle-host/src/view.rs`, `order` and
//! `value`): a `Timestamp` key ranks by instant only where both values parse, and by spelling
//! otherwise. ESS admits any text as a `Timestamp` (`Primitive::admits`), and the kernel stores
//! what ESS admits, so one unparsable value makes the comparison non-transitive, and two valid
//! timestamps come out in the wrong order.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"views-memory\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the in-memory environment parses")
}

#[test]
fn one_unparsable_timestamp_does_not_reorder_two_valid_ones() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/views");
    let loaded = spindle_spec::load(&fixture).expect("the views fixture compiles");
    let mut host = Host::new(
        loaded.ir,
        &in_memory(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the views fixture is admitted");
    // By instant: t3 (00:30Z) is newer than t1 (01:00+01:00 = 00:00Z).
    // By spelling: t1 sorts after t3, and t2 sorts between them.
    for (task_id, opened_at) in [
        ("t1", "2026-01-01T01:00:00+01:00"),
        ("t2", "2026-01-01T00:45 later"),
        ("t3", "2026-01-01T00:30:00Z"),
    ] {
        host.submit(
            "tasks.board.OpenTask",
            object(json!({"task_id": task_id, "title": task_id, "opened_at": opened_at})),
        )
        .unwrap_or_else(|error| panic!("{task_id} opens: {error}"));
    }
    let rows = host
        .list("tasks.board.NewestTasks")
        .expect("the list answers");
    let valid = rows
        .iter()
        .filter_map(|row| row.get("task_id").and_then(Value::as_str))
        .filter(|id| *id != "t2")
        .collect::<Vec<_>>();
    assert_eq!(
        valid,
        ["t3", "t1"],
        "newest first: 00:30Z before 00:00Z, whatever the third row holds; rows {rows:?}"
    );
}
