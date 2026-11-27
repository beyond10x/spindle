//! Adversary cases for wave 30 U6 (`story:decimal-identity-any-spelling`). A command now keys a
//! `Decimal` identity in one spelling (`1.0` as `1`), and the host's reads by identity outside a
//! command were left as they were: a view read by identity, a view field read through a
//! reference, and an instance's history. Each case reads an instance by the spelling the command
//! that created it was sent with; ESS reads every spelling of a number as one value, so each read
//! finds the instance.
//!
//! The model, inline: scales named by a `Decimal` weight, and pans that reference a scale by its
//! weight.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{HistoryCursor, Host, ManualClock};

const SYSTEM: &str = r"format: ess/34
system: probe
version: v1
summary: Scales named by a weight, and pans on them.
domains:
  - probe.pans
components:
  - component: kitchen
    summary: Keeps scales and pans.
    owns:
      domains:
        - probe.pans
    accepts:
      commands:
        - probe.pans.SetScale
        - probe.pans.AddPan
    publishes:
      events:
        - probe.pans.ScaleSet
        - probe.pans.PanAdded
    reached_by: in_process
";

const DOMAIN: &str = r"domain: probe.pans
summary: Scales named by a weight, and pans on them.
types: []
errors: []
entities:
  - name: probe.pans.Scale
    identity:
      name: grams
      type: Decimal
    fields:
      - name: note
        type: String
    lifecycle:
      initial: Set
      states: [Set]
      terminal: [Set]
  - name: probe.pans.Pan
    identity:
      name: pan_id
      type: String
    fields:
      - name: grams
        type: Optional<Decimal>
    relations:
      - {name: scale, kind: references, target: probe.pans.Scale, cardinality: one, via: grams}
    lifecycle:
      initial: Kept
      states: [Kept]
      terminal: [Kept]
commands:
  - name: probe.pans.SetScale
    input:
      - name: grams
        type: Decimal
      - name: note
        type: String
    outcomes:
      - name: set
        creates: probe.pans.Scale
        instance: grams
        sets:
          note: input.note
        emits: [probe.pans.ScaleSet]
        payload:
          probe.pans.ScaleSet: {grams: input.grams}
        summary: A scale is set at the weight.
  - name: probe.pans.AddPan
    input:
      - name: pan_id
        type: String
      - name: grams
        type: Optional<Decimal>
    outcomes:
      - name: added
        creates: probe.pans.Pan
        instance: pan_id
        sets:
          grams: input.grams
        emits: [probe.pans.PanAdded]
        payload:
          probe.pans.PanAdded: {pan_id: input.pan_id}
        summary: A pan is kept on the scale at the weight.
events:
  - name: probe.pans.ScaleSet
    fields:
      - name: grams
        type: Decimal
  - name: probe.pans.PanAdded
    fields:
      - name: pan_id
        type: String
views:
  - name: probe.pans.Scales
    source: probe.pans.Scale
    consistency: read_your_writes
    fields:
      - name: grams
        type: Decimal
      - name: note
        type: String
  - name: probe.pans.Pans
    source: probe.pans.Pan
    consistency: read_your_writes
    fields:
      - name: pan_id
        type: String
      - name: scale_note
        type: Optional<String>
        field: {related: {via: grams, field: note}}
";

const SCALE: &str = "probe.pans.Scale";
const SCALES: &str = "probe.pans.Scales";
const PANS: &str = "probe.pans.Pans";

fn ir() -> ess_compiler::EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/pans.yaml".to_owned()],
        &[SYSTEM.to_owned(), DOMAIN.to_owned()],
    )
    .expect("the inline model compiles")
}

fn memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"pans\"\n",
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

/// `text` parsed as JSON, so a number keeps the spelling it is written with.
fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|error| panic!("JSON {text}: {error}"))
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

/// A scale set at `grams`, spelled as written, with the note `top`.
fn set_scale(host: &mut Host, grams: &str) {
    host.submit(
        "probe.pans.SetScale",
        object(parsed(&format!(r#"{{"grams": {grams}, "note": "top"}}"#))),
    )
    .unwrap_or_else(|error| panic!("SetScale {grams} runs: {error}"));
}

/// A view field read through a reference: the pan names its scale `1.0`, the spelling the scale
/// was set with. ESS's interpreter loads the referenced row by value, and the base, which keyed
/// the scale `1.0`, found it; the scale is now keyed `1` and the view loads it by the field's
/// text `1.0`.
#[test]
fn a_view_reads_through_a_reference_spelled_as_the_scale_was_set() {
    let mut opened = host();
    set_scale(&mut opened.host, "1.0");
    opened
        .host
        .submit(
            "probe.pans.AddPan",
            object(parsed(r#"{"pan_id": "p1", "grams": 1.0}"#)),
        )
        .unwrap_or_else(|error| panic!("AddPan runs: {error}"));
    let pans = opened
        .host
        .listed_with(PANS, &Map::new())
        .unwrap_or_else(|error| panic!("{PANS} lists: {error}"))
        .rows;
    assert_eq!(
        pans,
        [object(json!({"pan_id": "p1", "scale_note": "top"}))],
        "the pan names the scale set at 1.0 by 1.0"
    );
}

/// `Host::view` by the identity a scale was set with, `1.0`: the base found it.
#[test]
fn a_view_reads_a_scale_by_the_spelling_it_was_set_with() {
    let mut opened = host();
    set_scale(&mut opened.host, "1.0");
    let read = opened
        .host
        .view(SCALES, &parsed("1.0"))
        .unwrap_or_else(|error| panic!("{SCALES} reads: {error}"));
    assert!(
        read.is_some_and(|row| row.get("note") == Some(&json!("top"))),
        "the scale set at 1.0 is read by 1.0"
    );
}

/// `Host::view` by the other spelling, `1`, of a scale set at `1.0`: ESS reads both as one value.
/// Kept: the row is keyed `1`.
#[test]
fn a_view_reads_a_scale_set_at_1_0_by_1() {
    let mut opened = host();
    set_scale(&mut opened.host, "1.0");
    let read = opened
        .host
        .view(SCALES, &json!(1))
        .unwrap_or_else(|error| panic!("{SCALES} reads: {error}"));
    assert!(
        read.is_some_and(|row| row.get("note") == Some(&json!("top"))),
        "the scale set at 1.0 is read by 1"
    );
}

/// The history of the scale set at `1.0`, asked by `1.0` (`GET /api/history?identity=1.0` reads
/// the identity as this JSON value): the base answered its `ScaleSet`; the write now names the
/// instance `1`, and the history compares the two values by their JSON spelling.
#[test]
fn a_history_reads_a_scale_by_the_spelling_it_was_set_with() {
    let mut opened = host();
    set_scale(&mut opened.host, "1.0");
    let history = opened
        .host
        .history(SCALE, &parsed("1.0"), &HistoryCursor::start(), usize::MAX)
        .unwrap_or_else(|error| panic!("the history reads: {error}"));
    let events = history
        .events
        .iter()
        .map(|revision| revision.event.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        ["probe.pans.ScaleSet"],
        "the scale set at 1.0 has its history at 1.0"
    );
}
