//! A command names a `Decimal` or `Binary64` identity whatever its spelling
//! (`story:decimal-identity-any-spelling`): ESS keys an instance by the value it reads and compares
//! numbers by their exact value whatever their declared type, so `1`, `1.0` and `1e0` name one
//! instance. The kernel keys a row by its identity written in one spelling (an integral number as
//! the integer, any other as the binary64 it reads), so the stores hold and scan it so. A `Decimal`
//! field beside the identity keeps its spelling in the row and the event payload.
//!
//! The model is written per test into a scratch directory under the cargo target directory.

use serde_json::{Value, json};
use spindle_kernel::{MemoryStore, Store};

use super::adversary_w27_u5::{agreed, identities, outcome, parsed, send};

const SYSTEM: &str = "format: ess/35
system: probe
version: v1
summary: Scales named by a Decimal weight, gauges named by a Binary64 reading.
domains:
  - probe.scale
components:
  - component: store
    summary: Sets and notes scales and gauges.
    owns:
      domains:
        - probe.scale
    accepts:
      commands:
        - probe.scale.Weigh
        - probe.scale.Note
        - probe.scale.Mount
        - probe.scale.Read
        - probe.scale.Sweep
    publishes:
      events:
        - probe.scale.Weighed
        - probe.scale.Noted
        - probe.scale.Swept
        - probe.scale.Mounted
        - probe.scale.GaugeRead
    reached_by: network
";

const DOMAIN: &str = "domain: probe.scale
summary: Scales named by a Decimal weight, gauges named by a Binary64 reading.
types: []
errors: []
entities:
  - name: probe.scale.Scale
    identity:
      name: grams
      type: Decimal
    fields:
      - name: tare
        type: Decimal
      - name: note
        type: Optional<String>
    lifecycle:
      initial: Set
      states: [Set]
      terminal: [Set]
  - name: probe.scale.Gauge
    identity:
      name: level
      type: Binary64
    fields:
      - name: note
        type: Optional<String>
    lifecycle:
      initial: Mounted
      states: [Mounted]
      terminal: [Mounted]
events:
  - name: probe.scale.Weighed
    fields:
      - name: grams
        type: Decimal
      - name: tare
        type: Decimal
  - name: probe.scale.Noted
    fields:
      - name: grams
        type: Decimal
  - name: probe.scale.Mounted
    fields:
      - name: level
        type: Binary64
  - name: probe.scale.GaugeRead
    fields:
      - name: level
        type: Binary64
  - name: probe.scale.Swept
    fields:
      - name: grams
        type: Decimal
commands:
  - name: probe.scale.Sweep
    input:
      - name: grams
        type: Decimal
    outcomes:
      - name: swept
        updates: probe.scale.Scale
        instance: grams
        sets:
          note: kept
        affects:
          - entity: probe.scale.Scale
            where: tare >= 0
            sets:
              note: swept
        emits: [probe.scale.Swept]
        payload:
          probe.scale.Swept:
            grams: input.grams
        summary: The scale at the weight is kept, and every other scale swept.
  - name: probe.scale.Weigh
    input:
      - name: grams
        type: Decimal
      - name: tare
        type: Decimal
    outcomes:
      - name: weighed
        creates: probe.scale.Scale
        instance: grams
        sets:
          tare: input.tare
        emits: [probe.scale.Weighed]
        payload:
          probe.scale.Weighed:
            grams: input.grams
            tare: input.tare
        summary: A scale is set at the weight, with its tare.
  - name: probe.scale.Note
    input:
      - name: grams
        type: Decimal
      - name: note
        type: String
    outcomes:
      - name: noted
        updates: probe.scale.Scale
        instance: grams
        sets:
          note: input.note
        emits: [probe.scale.Noted]
        payload:
          probe.scale.Noted:
            grams: input.grams
        summary: The scale at the weight is noted.
  - name: probe.scale.Mount
    input:
      - name: level
        type: Binary64
    outcomes:
      - name: mounted
        creates: probe.scale.Gauge
        instance: level
        emits: [probe.scale.Mounted]
        payload:
          probe.scale.Mounted:
            level: input.level
        summary: A gauge is mounted at the level.
  - name: probe.scale.Read
    input:
      - name: level
        type: Binary64
      - name: note
        type: String
    outcomes:
      - name: read
        updates: probe.scale.Gauge
        instance: level
        sets:
          note: input.note
        emits: [probe.scale.GaugeRead]
        payload:
          probe.scale.GaugeRead:
            level: input.level
        summary: The gauge at the level is read.
";

const SCALE: &str = "probe.scale.Scale";
const GAUGE: &str = "probe.scale.Gauge";
const WEIGH: &str = "probe.scale.Weigh";
const NOTE: &str = "probe.scale.Note";
const MOUNT: &str = "probe.scale.Mount";
const READ: &str = "probe.scale.Read";

/// The model, in a directory of its own.
fn ir() -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::write(dir.path().join("system.yaml"), SYSTEM).expect("the system writes");
    std::fs::write(dir.path().join("domains/scale.yaml"), DOMAIN).expect("the domain writes");
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the scale model compiles: {error}"))
        .ir;
    (dir, ir)
}

fn weigh(grams: &str, tare: &str) -> Value {
    parsed(&format!(r#"{{"grams": {grams}, "tare": {tare}}}"#))
}

fn note(grams: &str) -> Value {
    parsed(&format!(r#"{{"grams": {grams}, "note": "again"}}"#))
}

/// A scale set at `1.0` with a tare of `2.0`: the row and the event hold the tare as `2.0`, as
/// written, and the scale is held under the identity `1`.
#[test]
fn a_decimal_beside_a_decimal_identity_keeps_its_spelling() {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let answered = agreed(&ir, &mut store, &[(WEIGH, weigh("1.0", "2.0"))]);
    let weighed = answered[0].as_ref().expect("the scale is set");
    assert_eq!(weighed.outcome, "weighed");
    assert_eq!(
        identities(&mut store, SCALE),
        [json!(1)],
        "the identity reads 1"
    );
    let row = store
        .load(SCALE, &json!(1))
        .expect("the store reads")
        .expect("the scale is held under 1")
        .row;
    assert_eq!(
        row.fields.get("tare"),
        Some(&parsed("2.0")),
        "the row keeps the tare as `2.0`"
    );
    assert_eq!(
        weighed.events[0].payload.get("tare"),
        Some(&parsed("2.0")),
        "the event keeps the tare as `2.0`"
    );
}

/// A `Binary64` identity is keyed alike: `1.0`, `1`, `1e0`, `-0.0` and `0`, `5e-1` and `0.5`
/// each name one gauge, as ESS's interpreter reads each pair as one value.
#[test]
fn a_binary64_identity_names_one_row_whatever_its_spelling() {
    for (created, named) in [
        ("1.0", "1"),
        ("1", "1.0"),
        ("1e0", "1.0"),
        ("-0.0", "0"),
        ("0", "-0.0"),
        ("5e-1", "0.5"),
    ] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (MOUNT, parsed(&format!(r#"{{"level": {created}}}"#))),
                (
                    READ,
                    parsed(&format!(r#"{{"level": {named}, "note": "seen"}}"#)),
                ),
                (MOUNT, parsed(&format!(r#"{{"level": {named}}}"#))),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("mounted"), Some("read"), None],
            "a gauge mounted at {created} is found at {named}, and not mounted twice"
        );
        assert_eq!(
            identities(&mut store, GAUGE).len(),
            1,
            "one gauge, mounted at {created}"
        );
    }
}

/// An `affects:` entry over the subject's own entity leaves the subject out whatever the spelling
/// the command names it by: the scale set at `1` and swept as `1.0` is kept, the other swept.
#[test]
fn an_affects_entry_leaves_out_its_subject_whatever_its_spelling() {
    for (created, named) in [("1", "1.0"), ("1.0", "1e0")] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (WEIGH, weigh(created, "0")),
                (WEIGH, weigh("2", "0")),
                (
                    "probe.scale.Sweep",
                    parsed(&format!(r#"{{"grams": {named}}}"#)),
                ),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("weighed"), Some("weighed"), Some("swept")],
            "set at {created}, swept as {named}"
        );
        let notes = store
            .scan(SCALE)
            .expect("the store scans")
            .rows
            .into_iter()
            .map(|(identity, stored)| (identity, stored.row.fields.get("note").cloned()))
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            [
                (json!(1), Some(json!("kept"))),
                (json!(2), Some(json!("swept")))
            ],
            "set at {created}, swept as {named}"
        );
    }
}

/// A command naming a scale no row has refuses naming the identity as the caller sent it, `1.0`,
/// not the spelling the row would be keyed by.
#[test]
fn a_refusal_names_an_unknown_identity_as_it_was_sent() {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let refused = send(&ir, &mut store, NOTE, &note("1.0"))
        .expect_err("no scale is set, and Note declares no unknown_instance:");
    let refused = refused.to_string();
    assert!(
        refused.contains("has the identity 1.0,"),
        "the refusal names 1.0: {refused}"
    );
}

/// A durable SQLite store reopened between the two commands finds the scale set at `1.0` by `1`,
/// and the reverse.
#[cfg(feature = "sqlite")]
#[test]
fn a_durable_store_finds_a_decimal_identity_whatever_its_spelling() {
    use spindle_kernel::SqliteStore;
    for (created, named) in [("1.0", "1"), ("1", "1.0")] {
        let (dir, ir) = ir();
        let path = dir.path().join("scales.sqlite");
        {
            let mut store = SqliteStore::open(&path).expect("SQLite opens");
            let weighed =
                send(&ir, &mut store, WEIGH, &weigh(created, "2.0")).expect("the scale is set");
            assert_eq!(weighed.outcome, "weighed");
        }
        let mut store = SqliteStore::open(&path).expect("SQLite reopens");
        let noted = send(&ir, &mut store, NOTE, &note(named)).unwrap_or_else(|error| {
            panic!("a scale set at {created} is found at {named}: {error}")
        });
        assert_eq!(noted.outcome, "noted", "set at {created}, named {named}");
        assert_eq!(
            identities(&mut store, SCALE),
            [json!(1)],
            "one scale, keyed 1"
        );
    }
}
