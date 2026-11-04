//! A construct the kernel does not run is refused by name, never guessed at.

use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, execute};

const SYSTEM: &str = "format: ess/34
system: probe
version: v1
summary: A counter that only opens at a non-negative start and is never removed.
domains:
  - probe.counter
components:
  - component: probe
    summary: Holds counters.
    owns:
      domains:
        - probe.counter
    accepts:
      commands:
        - probe.counter.OpenCounter
        - probe.counter.RemoveCounter
    publishes:
      events:
        - probe.counter.CounterOpened
        - probe.counter.CounterRemoved
    reached_by: in_process
";

const COUNTER: &str = "domain: probe.counter
summary: One counter.
entities:
  - name: probe.counter.Counter
    identity:
      name: counter_id
      type: String
    fields:
      - name: value
        type: Integer
    lifecycle:
      initial: Open
      states: [Open]
      terminal: [Open]
errors:
  - name: probe.counter.NegativeStart
    summary: The start is below zero.
    fields: []
commands:
  - name: probe.counter.OpenCounter
    input:
      - name: counter_id
        type: String
      - name: start
        type: Integer
    outcomes:
      - name: negative
        when: start < 0
        error: probe.counter.NegativeStart
        summary: A negative start is refused.
      - name: opened
        creates: probe.counter.Counter
        instance: counter_id
        sets:
          value: input.start
        emits: [probe.counter.CounterOpened]
        payload:
          probe.counter.CounterOpened: {counter_id: input.counter_id}
        summary: The counter opens.
  - name: probe.counter.RemoveCounter
    input:
      - name: counter_id
        type: String
    outcomes:
      - name: removed
        deletes: probe.counter.Counter
        instance: counter_id
        emits: [probe.counter.CounterRemoved]
        payload:
          probe.counter.CounterRemoved: {counter_id: input.counter_id}
        summary: The counter is removed.
events:
  - name: probe.counter.CounterOpened
    fields:
      - name: counter_id
        type: String
  - name: probe.counter.CounterRemoved
    fields:
      - name: counter_id
        type: String
";

fn probe() -> spindle_spec::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/counter.yaml".to_owned()],
        &[SYSTEM.to_owned(), COUNTER.to_owned()],
    )
    .expect("the probe compiles")
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

#[test]
fn a_when_guard_over_the_input_refuses_before_anything_is_created() {
    let ir = probe();
    let mut store = MemoryStore::default();

    let refused = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "probe.counter.OpenCounter",
        &input(json!({"counter_id": "c1", "start": -1})),
    )
    .expect("a guarded refusal runs");
    assert_eq!(refused.outcome, "negative");
    assert!(refused.events.is_empty());
    assert_eq!(
        refused.error.map(|error| error.name),
        Some("probe.counter.NegativeStart".to_owned())
    );
    assert_eq!(
        store.load("probe.counter.Counter", &json!("c1")).unwrap(),
        None
    );

    let opened = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "probe.counter.OpenCounter",
        &input(json!({"counter_id": "c1", "start": 3})),
    )
    .expect("a start the guard admits opens the counter");
    assert_eq!(opened.outcome, "opened");
    let stored = store
        .load("probe.counter.Counter", &json!("c1"))
        .unwrap()
        .expect("the counter exists");
    assert_eq!(stored.row.fields.get("value"), Some(&json!(3)));
}

#[test]
fn a_deletion_is_refused_as_unsupported_and_changes_nothing() {
    let ir = probe();
    let mut store = MemoryStore::default();
    execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "probe.counter.OpenCounter",
        &input(json!({"counter_id": "c1", "start": 0})),
    )
    .expect("the counter opens");

    let error = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "probe.counter.RemoveCounter",
        &input(json!({"counter_id": "c1"})),
    )
    .expect_err("`deletes:` is not run");
    match &error {
        ExecuteError::Unsupported { construct } => {
            assert!(construct.contains("deletes"), "{construct}");
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert!(
        store
            .load("probe.counter.Counter", &json!("c1"))
            .unwrap()
            .is_some(),
        "the counter is still there"
    );
}

#[test]
fn an_unknown_command_is_named() {
    let ir = probe();
    let error = execute(
        &ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        "probe.counter.Nothing",
        &Map::new(),
    )
    .expect_err("no such command");
    assert!(
        matches!(&error, ExecuteError::UnknownCommand(name) if name == "probe.counter.Nothing"),
        "{error:?}"
    );
}
