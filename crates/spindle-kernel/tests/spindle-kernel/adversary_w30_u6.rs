//! Adversary cases for wave 30 U6 (`story:decimal-identity-any-spelling`): the spellings of a
//! `Decimal` identity the unit's tests leave out, each run on the kernel and on ESS's interpreter
//! step by step ([`agreed`]). Exponent forms, trailing zeroes, negative zero and the ends of the
//! 64-bit ranges name one scale where they spell one value; two values one binary64 apart, an
//! integer past 2^53 against its binary64, and `u64::MAX` against 2^64 name two.
//!
//! The model is written per test into a scratch directory under the cargo target directory.

use spindle_kernel::MemoryStore;

use super::adversary_w27_u5::{agreed, identities, outcome, parsed};

const SYSTEM: &str = "format: ess/35
system: probe
version: v1
summary: Scales named by a Decimal weight.
domains:
  - probe.weigh
components:
  - component: store
    summary: Sets and notes scales.
    owns:
      domains:
        - probe.weigh
    accepts:
      commands:
        - probe.weigh.Weigh
        - probe.weigh.Note
        - probe.weigh.AddPan
    publishes:
      events:
        - probe.weigh.Weighed
        - probe.weigh.Noted
        - probe.weigh.PanAdded
    reached_by: network
";

const DOMAIN: &str = "domain: probe.weigh
summary: Scales named by a Decimal weight.
types: []
errors:
  - name: probe.weigh.NoScale
    summary: No scale is set at the weight.
    fields: []
entities:
  - name: probe.weigh.Scale
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
  - name: probe.weigh.Pan
    identity:
      name: pan_id
      type: String
    fields:
      - name: grams
        type: Decimal
    relations:
      - {name: scale, kind: references, target: probe.weigh.Scale, cardinality: one, via: grams}
    lifecycle:
      initial: Kept
      states: [Kept]
      terminal: [Kept]
events:
  - name: probe.weigh.PanAdded
    fields:
      - name: pan_id
        type: String
  - name: probe.weigh.Weighed
    fields:
      - name: grams
        type: Decimal
  - name: probe.weigh.Noted
    fields:
      - name: grams
        type: Decimal
commands:
  - name: probe.weigh.Weigh
    input:
      - name: grams
        type: Decimal
    outcomes:
      - name: weighed
        creates: probe.weigh.Scale
        instance: grams
        emits: [probe.weigh.Weighed]
        payload:
          probe.weigh.Weighed:
            grams: input.grams
        summary: A scale is set at the weight.
  - name: probe.weigh.Note
    input:
      - name: grams
        type: Decimal
      - name: note
        type: String
    outcomes:
      - name: noted
        updates: probe.weigh.Scale
        instance: grams
        sets:
          note: input.note
        emits: [probe.weigh.Noted]
        payload:
          probe.weigh.Noted:
            grams: input.grams
        summary: The scale at the weight is noted.
  - name: probe.weigh.AddPan
    input:
      - name: pan_id
        type: String
      - name: grams
        type: Decimal
    outcomes:
      - name: no-scale
        when_related:
          via: input.grams
          exists: false
        error: probe.weigh.NoScale
        summary: No scale is set at the weight.
      - name: added
        creates: probe.weigh.Pan
        instance: pan_id
        sets:
          grams: input.grams
        emits: [probe.weigh.PanAdded]
        payload:
          probe.weigh.PanAdded:
            pan_id: input.pan_id
        summary: A pan is kept on the scale at the weight.
";

const SCALE: &str = "probe.weigh.Scale";
const WEIGH: &str = "probe.weigh.Weigh";
const NOTE: &str = "probe.weigh.Note";

/// The model, in a directory of its own.
fn ir() -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    std::fs::write(dir.path().join("system.yaml"), SYSTEM).expect("the system writes");
    std::fs::write(dir.path().join("domains/weigh.yaml"), DOMAIN).expect("the domain writes");
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the weigh model compiles: {error}"))
        .ir;
    (dir, ir)
}

/// Sets a scale at `created`, notes the scale at `named`, sets a scale at `named`: the outcome of
/// each step, agreed with ESS's interpreter, and the number of scales held after.
fn three_steps(created: &str, named: &str) -> (Vec<Option<String>>, usize) {
    let (_dir, ir) = ir();
    let mut store = MemoryStore::default();
    let answered = agreed(
        &ir,
        &mut store,
        &[
            (WEIGH, parsed(&format!(r#"{{"grams": {created}}}"#))),
            (
                NOTE,
                parsed(&format!(r#"{{"grams": {named}, "note": "seen"}}"#)),
            ),
            (WEIGH, parsed(&format!(r#"{{"grams": {named}}}"#))),
        ],
    );
    let outcomes = answered
        .iter()
        .map(|executed| outcome(executed).map(str::to_owned))
        .collect();
    (outcomes, identities(&mut store, SCALE).len())
}

/// Spellings of one value name one scale, on the kernel as on the interpreter: exponent forms in
/// either case, trailing zeroes, negative zero, a binary64 past `i64` and within `u64`, `i64::MIN`
/// as a binary64, and a token past binary64's precision that both read as the same binary64.
#[test]
fn every_spelling_of_one_value_names_one_scale() {
    for (created, named) in [
        ("10E-1", "1"),
        ("1.000", "1e0"),
        ("0.1e1", "1.0"),
        ("-0.0", "0"),
        ("-0", "0.0"),
        ("0.10", "1e-1"),
        ("2.5", "25E-1"),
        ("-1.0", "-1"),
        ("1e19", "10000000000000000000"),
        ("-9223372036854775808.0", "-9223372036854775808"),
        ("1", "1.00000000000000001"),
    ] {
        let (outcomes, held) = three_steps(created, named);
        assert_eq!(
            outcomes,
            [Some("weighed".to_owned()), Some("noted".to_owned()), None],
            "set at {created}, named {named}"
        );
        assert_eq!(held, 1, "one scale, set at {created}, named {named}");
    }
}

/// Values that differ name two scales, on the kernel as on the interpreter: two binary64s one
/// step apart, an integer past 2^53 against the binary64 its `.0` spelling reads as, and
/// `u64::MAX` against 2^64. Both scales are set: the second creation finds no scale at the first
/// weight. (A `Note` of an unknown scale is left out: the interpreter answers it with no outcome
/// where the kernel refuses it, whatever the spelling.)
#[test]
fn two_values_name_two_scales() {
    for (created, named) in [
        ("1", "1.0000000000000002"),
        ("0.1", "0.10000000000000002"),
        ("9007199254740993", "9007199254740993.0"),
        ("18446744073709551615", "18446744073709551616"),
        ("9223372036854775807", "9223372036854775808"),
    ] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (WEIGH, parsed(&format!(r#"{{"grams": {created}}}"#))),
                (WEIGH, parsed(&format!(r#"{{"grams": {named}}}"#))),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("weighed"), Some("weighed")],
            "set at {created}, then at {named}"
        );
        assert_eq!(
            identities(&mut store, SCALE).len(),
            2,
            "two scales, set at {created} and {named}"
        );
    }
}

/// A `when_related:` read of the scale a pan names by its weight finds the scale whatever the
/// spelling, on the kernel as on the interpreter. The read is by the input's value, not the
/// subject's, so only the store wrapper writes it in the one spelling: a kernel that spells the
/// subject's identity and hands the read the store unwrapped answers `no-scale`.
#[test]
fn a_related_read_finds_a_decimal_scale_whatever_its_spelling() {
    for (created, named) in [("1", "1.0"), ("1.0", "1e0"), ("10E-1", "1")] {
        let (_dir, ir) = ir();
        let mut store = MemoryStore::default();
        let answered = agreed(
            &ir,
            &mut store,
            &[
                (WEIGH, parsed(&format!(r#"{{"grams": {created}}}"#))),
                (
                    "probe.weigh.AddPan",
                    parsed(&format!(r#"{{"pan_id": "p1", "grams": {named}}}"#)),
                ),
            ],
        );
        assert_eq!(
            answered.iter().map(outcome).collect::<Vec<_>>(),
            [Some("weighed"), Some("added")],
            "set at {created}, the pan names {named}"
        );
    }
}
