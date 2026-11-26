//! Adversary cases for `story:integer-identity-any-spelling` (wave 27, unit U5): every `Integer`
//! of a command's input is written as the integer once ESS admitted it, whatever its spelling and
//! wherever it sits (a newtype identity, a struct member, a map value, a union payload under
//! either tag), so `1` and `1.0` address, hold and publish one value, as ESS's interpreter reads
//! both as one. A `Decimal` or `Binary64` is not an `Integer` and keeps its spelling. A reference
//! read through `when_related` and `{related: …}` finds its row by either spelling, and so does a
//! durable SQLite store reopened between the two commands.
//!
//! The model is written per test into a scratch directory under the cargo target directory.

use std::collections::BTreeMap;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, Executed, MemoryStore, NoExternals, Store, execute};

use super::related_guards::object;

const SYSTEM: &str = "format: ess/35
system: probe
version: v1
summary: Bins named by an Integer newtype, and slots that reference a bin.
domains:
  - probe.bin
components:
  - component: store
    summary: Opens bins and fills slots.
    owns:
      domains:
        - probe.bin
    accepts:
      commands:
        - probe.bin.OpenBin
        - probe.bin.TouchBin
        - probe.bin.FillSlot
        - probe.bin.Weigh
        - probe.bin.Reweigh
    publishes:
      events:
        - probe.bin.BinOpened
        - probe.bin.BinTouched
        - probe.bin.SlotFilled
        - probe.bin.Weighed
        - probe.bin.Reweighed
    reached_by: network
";

const DOMAIN: &str = "domain: probe.bin
summary: Bins named by an Integer newtype, holding Integers nested in every shape.
types:
  - name: probe.bin.BinNo
    kind: newtype
    of: Integer
  - name: probe.bin.Size
    kind: struct
    fields:
      - name: width
        type: Integer
      - name: depth
        type: Decimal
      - name: ratio
        type: Binary64
  - name: probe.bin.Mark
    kind: union
    tag: kind
    variants:
      count: Integer
      label: String
  - name: probe.bin.Lid
    kind: union
    tag: value
    variants:
      heavy: Integer
      none: null
errors:
  - name: probe.bin.NoBin
    summary: No bin carries the number.
    fields: []
entities:
  - name: probe.bin.Bin
    identity:
      name: bin_no
      type: probe.bin.BinNo
    fields:
      - name: size
        type: probe.bin.Size
      - name: tally
        type: Map<String, Integer>
      - name: mark
        type: probe.bin.Mark
      - name: lid
        type: probe.bin.Lid
      - name: weight
        type: Decimal
      - name: readings
        type: List<Decimal>
      - name: note
        type: Optional<String>
    lifecycle:
      initial: Open
      states: [Open]
      terminal: [Open]
  - name: probe.bin.Slot
    identity:
      name: slot_id
      type: String
    fields:
      - name: bin_no
        type: probe.bin.BinNo
      - name: weight
        type: Decimal
    relations:
      - {name: bin, kind: references, target: probe.bin.Bin, cardinality: one, via: bin_no}
    lifecycle:
      initial: Filled
      states: [Filled]
      terminal: [Filled]
  - name: probe.bin.Scale
    identity:
      name: grams
      type: Decimal
    fields:
      - name: note
        type: Optional<String>
    lifecycle:
      initial: Set
      states: [Set]
      terminal: [Set]
events:
  - name: probe.bin.BinOpened
    fields:
      - name: bin_no
        type: probe.bin.BinNo
      - name: size
        type: probe.bin.Size
      - name: tally
        type: Map<String, Integer>
      - name: mark
        type: probe.bin.Mark
      - name: lid
        type: probe.bin.Lid
      - name: readings
        type: List<Decimal>
  - name: probe.bin.BinTouched
    fields:
      - name: bin_no
        type: probe.bin.BinNo
  - name: probe.bin.SlotFilled
    fields:
      - name: slot_id
        type: String
      - name: bin_no
        type: probe.bin.BinNo
  - name: probe.bin.Weighed
    fields:
      - name: grams
        type: Decimal
  - name: probe.bin.Reweighed
    fields:
      - name: grams
        type: Decimal
commands:
  - name: probe.bin.OpenBin
    input:
      - name: bin_no
        type: probe.bin.BinNo
      - name: size
        type: probe.bin.Size
      - name: tally
        type: Map<String, Integer>
      - name: mark
        type: probe.bin.Mark
      - name: lid
        type: probe.bin.Lid
      - name: weight
        type: Decimal
      - name: readings
        type: List<Decimal>
    outcomes:
      - name: opened
        creates: probe.bin.Bin
        instance: bin_no
        sets:
          size: input.size
          tally: input.tally
          mark: input.mark
          lid: input.lid
          weight: input.weight
          readings: input.readings
        emits: [probe.bin.BinOpened]
        payload:
          probe.bin.BinOpened:
            bin_no: input.bin_no
            size: input.size
            tally: input.tally
            mark: input.mark
            lid: input.lid
            readings: input.readings
        summary: The bin is opened.
  - name: probe.bin.TouchBin
    input:
      - name: bin_no
        type: probe.bin.BinNo
      - name: note
        type: String
    outcomes:
      - name: touched
        updates: probe.bin.Bin
        instance: bin_no
        sets:
          note: input.note
        emits: [probe.bin.BinTouched]
        payload:
          probe.bin.BinTouched:
            bin_no: input.bin_no
        summary: The bin is touched.
  - name: probe.bin.FillSlot
    input:
      - name: slot_id
        type: String
      - name: bin_no
        type: probe.bin.BinNo
    outcomes:
      - name: no-bin
        when_related:
          via: input.bin_no
          exists: false
        error: probe.bin.NoBin
        summary: The named bin does not exist.
      - name: filled
        creates: probe.bin.Slot
        instance: slot_id
        sets:
          bin_no: input.bin_no
          weight: {related: {via: input.bin_no, field: weight}}
        emits: [probe.bin.SlotFilled]
        payload:
          probe.bin.SlotFilled:
            slot_id: input.slot_id
            bin_no: input.bin_no
        summary: The slot is filled from the named bin, copying its weight.
  - name: probe.bin.Weigh
    input:
      - name: grams
        type: Decimal
    outcomes:
      - name: weighed
        creates: probe.bin.Scale
        instance: grams
        emits: [probe.bin.Weighed]
        payload:
          probe.bin.Weighed:
            grams: input.grams
        summary: A scale is set at the weight.
  - name: probe.bin.Reweigh
    input:
      - name: grams
        type: Decimal
      - name: note
        type: String
    outcomes:
      - name: reweighed
        updates: probe.bin.Scale
        instance: grams
        sets:
          note: input.note
        emits: [probe.bin.Reweighed]
        payload:
          probe.bin.Reweighed:
            grams: input.grams
        summary: The scale at the weight is noted.
";

const BIN: &str = "probe.bin.Bin";
const SLOT: &str = "probe.bin.Slot";
const SCALE: &str = "probe.bin.Scale";
const OPEN: &str = "probe.bin.OpenBin";
const TOUCH: &str = "probe.bin.TouchBin";
const FILL: &str = "probe.bin.FillSlot";
const WEIGH: &str = "probe.bin.Weigh";
const REWEIGH: &str = "probe.bin.Reweigh";

/// The model, in a directory of its own.
fn ir() -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::write(dir.path().join("system.yaml"), SYSTEM).expect("the system writes");
    std::fs::write(dir.path().join("domains/bin.yaml"), DOMAIN).expect("the domain writes");
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the bin model compiles: {error}"))
        .ir;
    (dir, ir)
}

/// `text` parsed as JSON, so a number keeps the spelling it is written with.
fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("JSON {text}: {error}"))
}

/// An `OpenBin` input for `bin_no`, every nested `Integer` spelled `.0`, every `Decimal` and
/// `Binary64` spelled `.0` too.
fn open(bin_no: &str) -> Value {
    parsed(&format!(
        r#"{{"bin_no": {bin_no},
            "size": {{"width": 2.0, "depth": 2.0, "ratio": 2.0}},
            "tally": {{"a": 3.0}},
            "mark": {{"kind": "count", "value": 4.0}},
            "lid": {{"value": "heavy", "content": 5.0}},
            "weight": 6.0,
            "readings": [7.0]}}"#
    ))
}

fn touch(bin_no: &str) -> Value {
    parsed(&format!(r#"{{"bin_no": {bin_no}, "note": "seen"}}"#))
}

fn fill(bin_no: &str) -> Value {
    parsed(&format!(r#"{{"slot_id": "s1", "bin_no": {bin_no}}}"#))
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: &Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
}

/// ESS 0.99.1's interpreter's answer to each step, in one scenario: the outcome and the error it
/// names, or its refusal.
fn interpreted(
    ir: &spindle_spec::EssIr,
    steps: &[(&str, Value)],
) -> Vec<Result<(String, Option<String>), String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.bin/authored/integer-spelling"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("integer-spelling").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|(command, input)| {
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: object(input.clone())
                        .into_iter()
                        .map(|(field, value)| {
                            let node: Node = serde_json::from_value(value).expect("a node");
                            (field, node)
                        })
                        .collect::<BTreeMap<_, _>>(),
                    correlation: context.correlation.clone(),
                })
                .map(|executed| {
                    (
                        executed
                            .outcome
                            .map(|outcome| outcome.outcome.as_str().to_owned())
                            .unwrap_or_default(),
                        executed.error.map(|error| error.error.to_string()),
                    )
                })
                .map_err(|error| error.to_string())
        })
        .collect()
}

/// Runs `steps` on `store` and on ESS's interpreter, asserts each step answers alike (the same
/// outcome and error, or a refusal both), and answers the kernel's executions.
fn agreed(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    steps: &[(&str, Value)],
) -> Vec<Option<Executed>> {
    let theirs = interpreted(ir, steps);
    steps
        .iter()
        .zip(theirs)
        .enumerate()
        .map(|(step, ((command, input), theirs))| {
            let ours = send(ir, store, command, input);
            match (ours, theirs) {
                (Ok(ours), Ok((outcome, error))) => {
                    assert_eq!(
                        (
                            ours.outcome.clone(),
                            ours.error.as_ref().map(|error| error.name.clone())
                        ),
                        (outcome, error),
                        "step {step} {command} {input}"
                    );
                    Some(ours)
                }
                (Err(_), Err(_)) => None,
                (ours, theirs) => panic!(
                    "step {step} {command} {input}: the kernel answers {ours:?}, the interpreter \
                     {theirs:?}"
                ),
            }
        })
        .collect()
}

fn outcome(executed: &Option<Executed>) -> Option<&str> {
    executed.as_ref().map(|executed| executed.outcome.as_str())
}

fn identities(store: &mut dyn Store, entity: &str) -> Vec<Value> {
    store
        .scan(entity)
        .expect("the store scans")
        .rows
        .into_iter()
        .map(|(identity, _)| identity)
        .collect()
}

/// A newtype over `Integer` names one bin by `1.0`, `1`, `1e0`, `10E-1` and `0.1e1`, and `-0`
/// and `0` name one bin, as ESS reads each pair as one value.
#[test]
fn a_newtype_integer_identity_names_one_row_whatever_its_spelling() {
    for (created, named, held) in [
        ("1.0", "1", json!(1)),
        ("1", "1.0", json!(1)),
        ("1e0", "1", json!(1)),
        ("10E-1", "1", json!(1)),
        ("1", "0.1e1", json!(1)),
        ("-0", "0", json!(0)),
        ("0", "-0.0", json!(0)),
        ("-7.0", "-7", json!(-7)),
    ] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (OPEN, open(created)),
                (TOUCH, touch(named)),
                (OPEN, open(named)),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("opened"), Some("touched"), None],
            "a bin created as {created} is found as {named}, and not created twice"
        );
        assert_eq!(
            identities(&mut store, BIN),
            std::slice::from_ref(&held),
            "one bin, keyed {held}, created as {created}"
        );
        let touched = answered[1].as_ref().expect("the touch runs");
        assert_eq!(
            Value::Object(touched.events[0].payload.clone()),
            json!({"bin_no": held}),
            "the touch publishes the identity as the integer"
        );
    }
}

/// Every `Integer` nested in a struct, a map and a union (tag `kind`, payload `value`; tag
/// `value`, payload `content`) is held and published as the integer; a `Decimal` and a
/// `Binary64` beside it, and a `List<Decimal>`, keep their spelling.
#[test]
fn a_nested_integer_is_the_integer_and_a_decimal_keeps_its_spelling() {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let answered = agreed(&ir, &mut store, &[(OPEN, open("1.0"))]);
    let opened = answered[0].as_ref().expect("the bin opens");
    assert_eq!(opened.outcome, "opened");
    let row = store
        .load(BIN, &json!(1))
        .expect("the store reads")
        .expect("the bin is held under 1")
        .row;
    let held = serde_json::to_value(&row.fields).expect("fields serialize");
    assert_eq!(
        held["size"],
        parsed(r#"{"width": 2, "depth": 2.0, "ratio": 2.0}"#),
        "a struct's Integer member is the integer; its Decimal and Binary64 keep `2.0`"
    );
    assert_eq!(held["tally"], json!({"a": 3}), "a map's Integer value");
    assert_eq!(
        held["mark"],
        json!({"kind": "count", "value": 4}),
        "a union's Integer payload under `value`"
    );
    assert_eq!(
        held["lid"],
        json!({"value": "heavy", "content": 5}),
        "a union's Integer payload under `content`, where the tag is `value`"
    );
    assert_eq!(
        held["readings"],
        parsed("[7.0]"),
        "a List<Decimal> keeps `7.0`"
    );
    assert_eq!(
        Value::Object(opened.events[0].payload.clone()),
        parsed(
            r#"{"bin_no": 1,
                "size": {"width": 2, "depth": 2.0, "ratio": 2.0},
                "tally": {"a": 3},
                "mark": {"kind": "count", "value": 4},
                "lid": {"value": "heavy", "content": 5},
                "readings": [7.0]}"#
        ),
        "the event publishes the same values"
    );
}

/// `when_related: {via: input.bin_no, exists: false}` and `{related: {via: input.bin_no, …}}`
/// find the bin by either spelling of its number, as ESS's interpreter does.
#[test]
fn a_related_read_finds_its_row_whatever_the_integer_spelling() {
    for (created, named) in [("1", "1.0"), ("1.0", "1"), ("1", "1e0")] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[(OPEN, open(created)), (FILL, fill(named))],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("opened"), Some("filled")],
            "a bin created as {created} is related as {named}"
        );
        let slot = store
            .load(SLOT, &json!("s1"))
            .expect("the store reads")
            .expect("the slot is held")
            .row;
        assert_eq!(
            slot.fields["bin_no"],
            json!(1),
            "the reference is the integer"
        );
        assert!(
            slot.fields["weight"].as_f64() == Some(6.0),
            "the slot copies the bin's weight: {:?}",
            slot.fields.get("weight")
        );
    }
}

/// A durable SQLite store reopened between the two commands finds the bin created as `1.0` by
/// `1`, and the reverse.
#[cfg(feature = "sqlite")]
#[test]
fn a_durable_store_finds_an_integer_identity_whatever_its_spelling() {
    use spindle_kernel::SqliteStore;
    for (created, named) in [("1.0", "1"), ("1", "1.0")] {
        let (dir, ir) = ir();
        let path = dir.path().join("bins.sqlite");
        {
            let mut store = SqliteStore::open(&path).expect("SQLite opens");
            let opened = send(&ir, &mut store, OPEN, &open(created)).expect("the bin opens");
            assert_eq!(opened.outcome, "opened");
        }
        let mut store = SqliteStore::open(&path).expect("SQLite reopens");
        let touched = send(&ir, &mut store, TOUCH, &touch(named)).unwrap_or_else(|error| {
            panic!("a bin created as {created} is found as {named}: {error}")
        });
        assert_eq!(touched.outcome, "touched");
        assert_eq!(
            identities(&mut store, BIN),
            [json!(1)],
            "one bin, keyed as the integer"
        );
    }
}

/// ESS keys an instance by the value it reads and compares numbers by their exact value, so a
/// `Decimal` identity spelled `1.0` and `1` names one instance too. The story writes only an
/// `Integer` as the integer, so the kernel keys the two spellings apart.
#[test]
fn a_decimal_identity_names_one_row_whatever_its_spelling() {
    for (created, named) in [("1.0", "1"), ("1", "1.0")] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (WEIGH, parsed(&format!(r#"{{"grams": {created}}}"#))),
                (
                    REWEIGH,
                    parsed(&format!(r#"{{"grams": {named}, "note": "again"}}"#)),
                ),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("weighed"), Some("reweighed")],
            "a scale set at {created} is found at {named}"
        );
        assert_eq!(identities(&mut store, SCALE).len(), 1, "one scale");
    }
}
