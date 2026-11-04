//! A field an outcome leaves to the runtime, `{generated: true}`, that is not a minted creation
//! identity: the kernel mints a value of the field's declared type.
//!
//! The fixture `tests/fixtures/generated` starts a batch under the caller's id. Its serial (a
//! newtype of `String`), stamp (`Timestamp`), token (`Uuid`) and present note (`Optional<String>`)
//! are minted into the row, its sequence (`Integer`) into the event and stored from there with
//! `{payload: …}`, and its label is the input or, where none is sent, minted. Relabelling refuses
//! an empty label under a minted incident; weighing would mint a `Decimal`, which the runtime does
//! not.

use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{
    ExecuteError, Identity, MemoryStore, NoExternals, Store, Uuid, execute, execute_minting,
};

const BATCH: &str = "forge.batch.Batch";
const BATCH_ID: &str = "9b2f4c1e-7a3d-4e5f-8a6b-1c2d3e4f5a6b";

fn ir() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated");
    spindle_spec::load(&root)
        .expect("the generated fixture compiles")
        .ir
}

fn input(value: Value) -> Map<String, Value> {
    let Value::Object(input) = value else {
        panic!("an input is an object, not {value}")
    };
    input
}

/// The `n`th UUID a counting mint answers.
fn counted(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

/// A mint that answers [`counted`] UUIDs in turn and records every request.
struct Counting {
    next: u64,
    asked: Vec<String>,
}

impl Counting {
    fn new() -> Self {
        Self {
            next: 0,
            asked: Vec::new(),
        }
    }
}

impl spindle_kernel::Mint for Counting {
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self.next += 1;
        self.asked.push(format!(
            "{} for {}/{}: {}.{}",
            identity.capability,
            identity.command,
            identity.outcome,
            identity.entity,
            identity.field
        ));
        Uuid::parse_str(&counted(self.next)).expect("a UUID")
    }
}

fn row(store: &mut MemoryStore) -> Map<String, Value> {
    let stored = store
        .load(BATCH, &json!(BATCH_ID))
        .expect("the store reads")
        .expect("the batch is stored");
    stored.row.fields.into_iter().collect()
}

fn start(store: &mut MemoryStore, mint: &mut Counting, label: Option<&str>) -> Value {
    let mut sent = json!({"batch_id": BATCH_ID});
    if let Some(label) = label {
        sent["label"] = json!(label);
    }
    let started = execute_minting(
        &ir(),
        store,
        &mut NoExternals,
        mint,
        "forge.batch.StartBatch",
        &input(sent),
    )
    .expect("a creation with generated fields runs");
    assert_eq!(started.outcome, "started");
    assert_eq!(started.error, None);
    assert_eq!(started.events.len(), 1, "{:?}", started.events);
    assert_eq!(started.events[0].name, "forge.batch.BatchStarted");
    Value::Object(started.events[0].payload.clone())
}

fn is_rfc3339_utc(text: &str) -> bool {
    let bytes = text.as_bytes();
    text.len() >= 20
        && text.ends_with('Z')
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
}

#[test]
fn a_generated_stored_field_is_minted_and_kept() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let mut mint = Counting::new();
    start(&mut store, &mut mint, Some("kiln-7"));

    let started = row(&mut store);
    let serial = started["serial"].as_str().expect("a minted serial is text");
    assert!(
        Uuid::parse_str(serial).is_ok(),
        "a minted `String` newtype is a fresh UUID's text: {serial}"
    );
    let token = started["token"].as_str().expect("a minted token is text");
    assert!(Uuid::parse_str(token).is_ok(), "{token} is a Uuid");
    assert_ne!(serial, token, "each generated field has a value of its own");
    let fired_at = started["fired_at"]
        .as_str()
        .expect("a minted stamp is text");
    assert!(
        is_rfc3339_utc(fired_at),
        "{fired_at} is an RFC 3339 instant"
    );
    let note = started["note"].as_str().expect("a present mint is present");
    assert!(Uuid::parse_str(note).is_ok(), "{note}");
    assert!(
        started["sequence"].is_i64(),
        "a minted `Integer` is an integer: {}",
        started["sequence"]
    );
    assert_eq!(
        mint.asked,
        vec![
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.serial",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.BatchStarted.sequence",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.token",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.note",
        ],
        "the mint is asked once per minted field, the event's sequence once for the event and \
         the row together"
    );

    let relabelled = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "forge.batch.RelabelBatch",
        &input(json!({"batch_id": BATCH_ID, "label": "kiln-8"})),
    )
    .expect("the batch relabels");
    assert_eq!(relabelled.outcome, "relabelled");
    let kept = row(&mut store);
    assert_eq!(kept["label"], json!("kiln-8"));
    for field in ["serial", "sequence", "fired_at", "token", "note"] {
        assert_eq!(
            kept[field], started[field],
            "`{field}` keeps its minted value"
        );
    }
}

#[test]
fn a_generated_payload_field_matches_the_stored_one() {
    let mut store = MemoryStore::default();
    let mut mint = Counting::new();
    let payload = start(&mut store, &mut mint, Some("kiln-7"));

    let published = payload["sequence"].clone();
    assert!(
        published.is_i64(),
        "the event carries a minted Integer: {payload}"
    );
    assert_eq!(payload["batch_id"], json!(BATCH_ID));
    assert_eq!(
        row(&mut store)["sequence"],
        published,
        "`{{payload: BatchStarted.sequence}}` stores the value the event published"
    );

    let mut other = MemoryStore::default();
    let again = start(&mut other, &mut mint, Some("kiln-7"));
    assert_ne!(
        again["sequence"], published,
        "a second start mints a sequence of its own"
    );
}

#[test]
fn an_input_value_wins_over_the_generated_else() {
    let mut sent = MemoryStore::default();
    let mut mint = Counting::new();
    start(&mut sent, &mut mint, Some("kiln-7"));
    assert_eq!(
        row(&mut sent)["label"],
        json!("kiln-7"),
        "the input is stored"
    );
    assert!(
        !mint
            .asked
            .iter()
            .any(|asked| asked.ends_with("forge.batch.Batch.label")),
        "nothing is minted for a label the input sends: {:?}",
        mint.asked
    );

    let mut absent = MemoryStore::default();
    let mut mint = Counting::new();
    start(&mut absent, &mut mint, None);
    let label = row(&mut absent)["label"].clone();
    let label = label.as_str().expect("an absent label is minted as text");
    assert!(Uuid::parse_str(label).is_ok(), "{label}");
    assert!(
        mint.asked
            .iter()
            .any(|asked| asked.ends_with("forge.batch.Batch.label")),
        "the label is minted where the input sends none: {:?}",
        mint.asked
    );

    let mut null = MemoryStore::default();
    let mut mint = Counting::new();
    execute_minting(
        &ir(),
        &mut null,
        &mut NoExternals,
        &mut mint,
        "forge.batch.StartBatch",
        &input(json!({"batch_id": BATCH_ID, "label": null})),
    )
    .expect("a `null` label is no label");
    let label = row(&mut null)["label"].clone();
    assert!(
        label
            .as_str()
            .is_some_and(|label| Uuid::parse_str(label).is_ok()),
        "a `null` label is minted: {label}"
    );
}

#[test]
fn a_generated_value_of_an_unmintable_type_is_refused_by_name() {
    let ir = ir();
    let mut store = MemoryStore::default();
    start(&mut store, &mut Counting::new(), Some("kiln-7"));
    let before = row(&mut store);

    match execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "forge.batch.WeighBatch",
        &input(json!({"batch_id": BATCH_ID})),
    ) {
        Err(ExecuteError::Unsupported { construct }) => {
            assert!(
                construct.contains("`weight`"),
                "names the field: {construct}"
            );
            assert!(construct.contains("Decimal"), "names the type: {construct}");
            assert!(
                construct.contains("forge.batch.WeighBatch/weighed"),
                "names the outcome: {construct}"
            );
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert_eq!(row(&mut store), before, "a refused command changes nothing");
}

#[test]
fn a_generated_error_field_is_minted() {
    let ir = ir();
    let mut store = MemoryStore::default();
    start(&mut store, &mut Counting::new(), Some("kiln-7"));
    let refused = execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "forge.batch.RelabelBatch",
        &input(json!({"batch_id": BATCH_ID, "label": ""})),
    )
    .expect("an empty label is answered");
    assert_eq!(refused.outcome, "blank");
    let error = refused.error.expect("the refusal reports its error");
    assert_eq!(error.name, "forge.batch.BlankLabel");
    let incident = error.fields["incident"]
        .as_str()
        .expect("the incident is minted");
    assert!(Uuid::parse_str(incident).is_ok(), "{incident}");
    assert_eq!(row(&mut store)["label"], json!("kiln-7"));
}
