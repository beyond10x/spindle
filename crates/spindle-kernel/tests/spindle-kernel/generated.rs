//! A field an outcome leaves to the runtime, `{generated: true}`, that is not a minted creation
//! identity: the kernel mints a value of the field's declared type.
//!
//! The fixture `tests/fixtures/generated` starts a batch under the caller's id. Its serial (a
//! newtype of `String`), stamp (`Timestamp`), token (`Uuid`) and present note (`Optional<String>`)
//! are minted into the row, its sequence (`Integer`) into the event and stored from there with
//! `{payload: …}`, and its label is the input or, where none is sent, minted. Relabelling refuses
//! an empty label under a minted incident; weighing would mint a `Decimal`, which the runtime does
//! not.
//!
//! The fixture `tests/fixtures/generated_boolean` installs a lamp whose `lit` flag (`Boolean`) the
//! event mints and the row stores from there, and arms it with a present minted
//! `Optional<Boolean>`. Its cases run the same model on ESS's own interpreter and compare.
//!
//! The fixture `tests/fixtures/generated_bytes` is a warehouse whose `PrintLabel` creates a
//! label with a minted `barcode` (`Bytes`, published and stored from the event) and minted
//! `tags` (`List<String>`), and whose `ReprintLabel` mints the barcode again. Its cases, too, run
//! on ESS's own interpreter and compare.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    ExecuteError, Identity, MemoryStore, NoExternals, Store, Uuid, UuidV4, execute, execute_minting,
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
            "identity.mint for forge.batch.StartBatch/started: forge.batch.BatchStarted.sequence",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.serial",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.token",
            "identity.mint for forge.batch.StartBatch/started: forge.batch.Batch.note",
        ],
        "the mint is asked once per minted field, the event's sequence once for the event and \
         the row together, and first, as ESS resolves a published value `sets:` stores before \
         the other `sets:` (`interpret::execute::take`, `publish_reads`)"
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

const LAMP: &str = "beacon.lamp.Lamp";
const LAMP_ID: &str = "4d1c9a7e-2b3f-4a5d-9e8f-0a1b2c3d4e5f";

fn lamps() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated_boolean");
    spindle_spec::load(&root)
        .expect("the generated Boolean fixture compiles")
        .ir
}

/// What ESS 0.87.0's interpreter does with the lamp fixture: installs `LAMP_ID`, then arms it,
/// answering the `lit` the install published and the row `LampCard` reads after each command.
///
/// `Interpreted::for_model` mints every value the model leaves to the implementation from one
/// counter per scenario (`interpret::execute::minted`, `interpret::values::mint`): a `Boolean` is
/// the witness of the tick drawn, `BASE_BOOL != (tick % 2 == 1)`
/// (`ess-conformance/src/witness.rs`, `primitive_value`), so `false` on tick 1, `true` on tick 2.
fn interpreted(ir: &spindle_spec::EssIr) -> (Node, ViewRowPair) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "beacon.lamp/authored/generated-boolean"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("generated-boolean").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let run = |command: &str| {
        target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: BTreeMap::from([("lamp_id".to_owned(), Node::Text(LAMP_ID.to_owned()))]),
                correlation: context.correlation.clone(),
            })
            .expect("the interpreter runs the command")
    };
    let card = || {
        let rows = target
            .query_view(SemanticViewRequest {
                view: ViewRef::new("beacon.lamp.LampCard".parse().expect("a view name")),
                params: BTreeMap::new(),
                consistency: ess_primitives::consistency::QueryConsistency::Current,
                correlation: context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the interpreter reads the view")
            .rows;
        assert_eq!(rows.len(), 1, "one lamp: {rows:?}");
        rows.into_iter().next().expect("one row")
    };
    let installed = run("beacon.lamp.InstallLamp");
    assert_eq!(installed.direct_events.len(), 1, "{installed:?}");
    let lit = installed.direct_events[0].payload["lit"].clone();
    let after_install = card();
    let armed = run("beacon.lamp.ArmLamp");
    assert!(armed.outcome.is_some(), "the interpreter arms: {armed:?}");
    (lit, (after_install, card()))
}

type ViewRowPair = (BTreeMap<String, Node>, BTreeMap<String, Node>);

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// A mint that answers the UUID ESS's interpreter mints on each tick in turn: the
/// `values::mint` of a `Uuid` on tick 1, 2, … (`00000000-0000-4000-8000-<tick>`), the same
/// counter the interpreter draws a `Boolean` from.
struct Ticks {
    ir: spindle_spec::EssIr,
    tick: u64,
    asked: Vec<String>,
}

impl spindle_kernel::Mint for Ticks {
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self.tick += 1;
        self.asked
            .push(format!("{}.{}", identity.entity, identity.field));
        let uuid = ess_compiler::ir::ResolvedTypeRef::Primitive {
            name: ess_domain::types::Primitive::Uuid,
        };
        match ess_conformance::interpret::values::mint(&self.ir, &uuid, self.tick) {
            Ok(Some(Node::Text(text))) => Uuid::parse_str(&text).expect("ESS mints a UUID"),
            other => panic!("ESS mints a UUID on tick {}: {other:?}", self.tick),
        }
    }
}

fn lamp_row(store: &mut MemoryStore) -> Map<String, Value> {
    let stored = store
        .load(LAMP, &json!(LAMP_ID))
        .expect("the store reads")
        .expect("the lamp is stored");
    stored.row.fields.into_iter().collect()
}

#[test]
fn a_generated_boolean_is_minted_as_ess_mints_it() {
    let ir = lamps();
    let (ess_lit, (ess_installed, ess_armed)) = interpreted(&ir);
    assert_eq!(
        ess_lit,
        Node::Bool(false),
        "ESS 0.87.0 mints `false` on tick 1"
    );

    let mut store = MemoryStore::default();
    let mut mint = Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    };
    let sent = input(json!({"lamp_id": LAMP_ID}));
    let installed = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "beacon.lamp.InstallLamp",
        &sent,
    )
    .expect("a generated Boolean is minted, not refused");
    assert_eq!(installed.outcome, "installed");
    assert_eq!(installed.events.len(), 1, "{:?}", installed.events);
    assert_eq!(
        installed.events[0].payload["lit"],
        json_of(&ess_lit),
        "the published Boolean is the one ESS's interpreter publishes"
    );
    let row = lamp_row(&mut store);
    assert_eq!(
        row["lit"],
        json_of(&ess_installed["lit"]),
        "the stored Boolean is the one ESS's interpreter stores"
    );
    assert_eq!(
        row["lit"], installed.events[0].payload["lit"],
        "`{{payload: LampInstalled.lit}}` stores what the event published"
    );

    let armed = execute_minting(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut mint,
        "beacon.lamp.ArmLamp",
        &sent,
    )
    .expect("a present generated Optional<Boolean> is minted");
    assert_eq!(armed.outcome, "armed");
    let row = lamp_row(&mut store);
    assert_eq!(
        row["armed"],
        json_of(&ess_armed["armed"]),
        "the second draw is ESS's tick 2"
    );
    assert_eq!(row["armed"], json!(true));
    assert_eq!(row["lit"], json_of(&ess_armed["lit"]), "`lit` is kept");
    assert_eq!(
        mint.asked,
        vec!["beacon.lamp.LampInstalled.lit", "beacon.lamp.Lamp.armed"],
        "one draw per minted Boolean, as ESS draws one tick per value"
    );
}

#[test]
fn derivation_names_the_capability_a_generated_boolean_needs() {
    use spindle_capability::{Capability, Reason, derive};

    let ir = lamps();
    let requirements = derive(&ir, Some("tower")).expect("tower is a component");
    let reason = |outcome: &str, because: &str| Reason {
        kind: "outcome",
        name: outcome.to_owned(),
        because: because.to_owned(),
    };
    assert_eq!(
        requirements
            .get(&Capability::new(Capability::IDENTITY_MINT))
            .map(|reasons| reasons.iter().cloned().collect::<Vec<_>>()),
        Some(vec![
            reason(
                "beacon.lamp.ArmLamp/armed",
                "mints `beacon.lamp.Lamp.armed`"
            ),
            reason(
                "beacon.lamp.InstallLamp/installed",
                "mints `beacon.lamp.LampInstalled.lit`"
            ),
            reason(
                "beacon.lamp.SignalLamp/signalled",
                "mints `beacon.lamp.LampSignalled.first`"
            ),
            reason(
                "beacon.lamp.SignalLamp/signalled",
                "mints `beacon.lamp.LampSignalled.second`"
            ),
        ]),
        "ESS draws a Boolean from the counter it mints identities from, so the kernel draws it \
         from `identity.mint`; the stored `{{payload: …}}` copy mints nothing of its own"
    );
    assert_eq!(
        requirements.get(&Capability::new(Capability::CLOCK)),
        None,
        "a Boolean reads no clock"
    );
}

/// ESS's `emit` walks an event's declared fields, not the payload as written
/// (`interpret::execute::emit`, `for field in &declared.fields`): `SignalLamp` writes `second`
/// before `first`, and ESS draws `first` on tick 2 (`true`) and `second` on tick 3 (`false`).
/// The kernel walks the compiled payload, which already agrees: a guard against a reordering.
#[test]
fn generated_payload_fields_are_drawn_in_the_events_declared_order_as_ess_draws_them() {
    let ir = lamps();
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "beacon.lamp/authored/declared-order"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("declared-order").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let commands = ["beacon.lamp.InstallLamp", "beacon.lamp.SignalLamp"];
    let mut ess = Vec::new();
    for command in commands {
        let result = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: BTreeMap::from([("lamp_id".to_owned(), Node::Text(LAMP_ID.to_owned()))]),
                correlation: context.correlation.clone(),
            })
            .expect("the interpreter runs the command");
        assert_eq!(result.direct_events.len(), 1, "{result:?}");
        ess.push(result.direct_events[0].payload.clone());
    }
    assert_eq!(
        (&ess[1]["first"], &ess[1]["second"]),
        (&Node::Bool(true), &Node::Bool(false)),
        "ESS: `first` on tick 2, `second` on tick 3"
    );

    let mut store = MemoryStore::default();
    let mut mint = Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    };
    let sent = input(json!({"lamp_id": LAMP_ID}));
    let mut published = Vec::new();
    for command in commands {
        let executed =
            execute_minting(&ir, &mut store, &mut NoExternals, &mut mint, command, &sent)
                .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
        published.push(executed.events[0].payload.clone());
    }
    assert_eq!(
        (
            published[1]["first"].clone(),
            published[1]["second"].clone()
        ),
        (json_of(&ess[1]["first"]), json_of(&ess[1]["second"])),
        "the kernel draws `first` and `second` on ESS's ticks"
    );
    assert_eq!(
        mint.asked,
        vec![
            "beacon.lamp.LampInstalled.lit",
            "beacon.lamp.LampSignalled.first",
            "beacon.lamp.LampSignalled.second",
        ]
    );
}

const LABEL: &str = "depot.warehouse.Label";
const LABEL_ID: &str = "6e2a8b4c-1d3f-4b5a-8c7d-9e0f1a2b3c4d";

fn labels() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generated_bytes");
    spindle_spec::load(&root)
        .expect("the generated Bytes fixture compiles")
        .ir
}

/// What ESS 0.87.0's interpreter publishes and answers for the warehouse fixture.
struct Warehouse {
    /// The `barcode` `LabelPrinted` published.
    printed: Node,
    /// The `LabelCard` row after `PrintLabel`.
    after_print: BTreeMap<String, Node>,
    /// The `LabelCard` row after `ReprintLabel`.
    after_reprint: BTreeMap<String, Node>,
}

/// Prints `LABEL_ID` on ESS 0.87.0's interpreter, then reprints it.
///
/// ESS draws every value the model leaves to the implementation from one counter per scenario,
/// advanced before each draw (`ess-conformance/src/interpret/execute.rs`, `minted`, line 2828:
/// `super::values::mint(ir, target, tick())`, line 2836), and resolves each `{payload: …}` a
/// `sets:` stores before the other `sets:` (`take`, `publish_reads`). So `PrintLabel` draws the
/// published `barcode` on tick 1 and the stored `tags` on tick 2, and `ReprintLabel` its
/// `barcode` on tick 3.
///
/// A value other than a `Uuid` is the witness synthesis builds for the tick-th further instance
/// (`interpret/values.rs`, `mint`, lines 60-93: `witness::fields` at `Distinction::further`):
///
/// - a `Bytes` is one byte, `BASE_BYTE` (0, `witness.rs` line 5800) plus the tick's low byte
///   (`primitive_value`, lines 6562 and 6577), padded base64 (`base64_byte`, line 6821): `AQ==`
///   on tick 1, `Aw==` on tick 3;
/// - a `List` is `[]` (`Builder::list`, lines 6124-6145): `fields` builds with nothing to expand
///   and nothing copied (line 1093), so no element is built, and the draw still takes its tick.
fn warehouse(ir: &spindle_spec::EssIr) -> Warehouse {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "depot.warehouse/authored/generated-bytes"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("generated-bytes").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let run = |command: &str| {
        target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: BTreeMap::from([("label_id".to_owned(), Node::Text(LABEL_ID.to_owned()))]),
                correlation: context.correlation.clone(),
            })
            .expect("the interpreter runs the command")
    };
    let card = || {
        let rows = target
            .query_view(SemanticViewRequest {
                view: ViewRef::new("depot.warehouse.LabelCard".parse().expect("a view name")),
                params: BTreeMap::new(),
                consistency: ess_primitives::consistency::QueryConsistency::Current,
                correlation: context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the interpreter reads the view")
            .rows;
        assert_eq!(rows.len(), 1, "one label: {rows:?}");
        rows.into_iter().next().expect("one row")
    };
    let printed = run("depot.warehouse.PrintLabel");
    assert_eq!(printed.direct_events.len(), 1, "{printed:?}");
    let barcode = printed.direct_events[0].payload["barcode"].clone();
    let after_print = card();
    let reprinted = run("depot.warehouse.ReprintLabel");
    assert!(
        reprinted.outcome.is_some(),
        "the interpreter reprints: {reprinted:?}"
    );
    Warehouse {
        printed: barcode,
        after_print,
        after_reprint: card(),
    }
}

fn label_row(store: &mut MemoryStore) -> Map<String, Value> {
    let stored = store
        .load(LABEL, &json!(LABEL_ID))
        .expect("the store reads")
        .expect("the label is stored");
    stored.row.fields.into_iter().collect()
}

fn ticks(ir: &spindle_spec::EssIr) -> Ticks {
    Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    }
}

/// Runs `command` on the kernel against `LABEL_ID`, drawing from `mint`.
fn label_command(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    mint: &mut Ticks,
    command: &str,
) -> spindle_kernel::Executed {
    execute_minting(
        ir,
        store,
        &mut NoExternals,
        mint,
        command,
        &input(json!({"label_id": LABEL_ID})),
    )
    .unwrap_or_else(|error| panic!("{command} runs: {error:?}"))
}

#[test]
fn a_generated_bytes_value_is_minted_as_ess_mints_it() {
    let ir = labels();
    let ess = warehouse(&ir);
    assert_eq!(
        ess.printed,
        Node::Text("AQ==".to_owned()),
        "ESS 0.87.0 mints the one byte 0x01, padded base64, on tick 1"
    );

    let mut store = MemoryStore::default();
    let mut mint = ticks(&ir);
    let printed = label_command(&ir, &mut store, &mut mint, "depot.warehouse.PrintLabel");
    assert_eq!(printed.outcome, "printed");
    assert_eq!(printed.events.len(), 1, "{:?}", printed.events);
    assert_eq!(
        printed.events[0].payload["barcode"],
        json_of(&ess.printed),
        "the published Bytes is the one ESS's interpreter publishes"
    );
    let row = label_row(&mut store);
    assert_eq!(
        row["barcode"],
        json_of(&ess.after_print["barcode"]),
        "the stored Bytes is the one ESS's interpreter stores"
    );
    assert_eq!(
        row["barcode"], printed.events[0].payload["barcode"],
        "`{{payload: LabelPrinted.barcode}}` stores what the event published"
    );
}

#[test]
fn a_generated_list_value_is_minted_as_ess_mints_it() {
    let ir = labels();
    let ess = warehouse(&ir);
    assert_eq!(
        ess.after_print["tags"],
        Node::Seq(Vec::new()),
        "ESS 0.87.0 mints a generated `List<String>` as `[]`"
    );

    let mut store = MemoryStore::default();
    let mut mint = ticks(&ir);
    let printed = label_command(&ir, &mut store, &mut mint, "depot.warehouse.PrintLabel");
    assert_eq!(printed.outcome, "printed");
    assert_eq!(
        label_row(&mut store)["tags"],
        json_of(&ess.after_print["tags"]),
        "the stored list is the one ESS's interpreter stores"
    );
    assert_eq!(
        mint.asked,
        vec![
            "depot.warehouse.LabelPrinted.barcode",
            "depot.warehouse.Label.tags"
        ],
        "the list takes a draw of its own after the published barcode, as ESS takes tick 2 for it"
    );
}

/// A generated `Bytes` differs between two mints. A generated `List` does not: ESS mints `[]` on
/// every tick (see [`warehouse`]), so two minted lists are equal, and the kernel's are too.
#[test]
fn two_mints_differ() {
    let ir = labels();
    let ess = warehouse(&ir);
    assert_eq!(
        ess.after_reprint["barcode"],
        Node::Text("Aw==".to_owned()),
        "ESS 0.87.0 reprints on tick 3, after the list's tick 2"
    );

    let mut store = MemoryStore::default();
    let mut mint = ticks(&ir);
    label_command(&ir, &mut store, &mut mint, "depot.warehouse.PrintLabel");
    let first = label_row(&mut store);
    let reprinted = label_command(&ir, &mut store, &mut mint, "depot.warehouse.ReprintLabel");
    assert_eq!(reprinted.outcome, "reprinted");
    let second = label_row(&mut store);
    assert_ne!(
        first["barcode"], second["barcode"],
        "a second mint draws a barcode of its own"
    );
    assert_eq!(
        second["barcode"],
        json_of(&ess.after_reprint["barcode"]),
        "the reprint draws ESS's tick 3"
    );
    assert_eq!(
        second["tags"],
        json_of(&ess.after_reprint["tags"]),
        "the minted tags are kept"
    );

    let mut fifth = MemoryStore::default();
    let mut sixth = MemoryStore::default();
    let print = |store: &mut MemoryStore, mint: &mut dyn spindle_kernel::Mint| {
        execute_minting(
            &ir,
            store,
            &mut NoExternals,
            mint,
            "depot.warehouse.PrintLabel",
            &input(json!({"label_id": LABEL_ID})),
        )
        .expect("a label prints")
    };
    let mut first = ticks(&ir);
    first.tick = 4;
    print(&mut fifth, &mut first);
    let mut later = ticks(&ir);
    later.tick = 5;
    print(&mut sixth, &mut later);
    assert_ne!(
        label_row(&mut fifth)["barcode"],
        label_row(&mut sixth)["barcode"],
        "two prints on different ticks mint different barcodes"
    );
    assert_eq!(
        label_row(&mut fifth)["tags"],
        label_row(&mut sixth)["tags"],
        "and the same `[]`, as ESS mints on every tick"
    );

    // The production mint, `uuid-v4`: one byte has 256 values, so any two random mints collide
    // one time in 256. Over 32 prints, all equal happens with probability 256^-31.
    let mut barcodes = BTreeSet::new();
    let mut lists = BTreeSet::new();
    for _ in 0..32 {
        let mut store = MemoryStore::default();
        print(&mut store, &mut UuidV4);
        let row = label_row(&mut store);
        barcodes.insert(row["barcode"].to_string());
        lists.insert(row["tags"].to_string());
    }
    assert!(
        barcodes.len() > 1,
        "random mints draw different barcodes: {barcodes:?}"
    );
    assert_eq!(
        lists,
        BTreeSet::from(["[]".to_owned()]),
        "and `[]` every time"
    );
}

#[test]
fn derivation_names_the_capability_these_mints_need() {
    use spindle_capability::{Capability, Reason, derive};

    let ir = labels();
    let requirements = derive(&ir, Some("printer")).expect("printer is a component");
    let reason = |outcome: &str, because: &str| Reason {
        kind: "outcome",
        name: outcome.to_owned(),
        because: because.to_owned(),
    };
    assert_eq!(
        requirements
            .get(&Capability::new(Capability::IDENTITY_MINT))
            .map(|reasons| reasons.iter().cloned().collect::<Vec<_>>()),
        Some(vec![
            reason(
                "depot.warehouse.PrintLabel/printed",
                "mints `depot.warehouse.Label.tags`"
            ),
            reason(
                "depot.warehouse.PrintLabel/printed",
                "mints `depot.warehouse.LabelPrinted.barcode`"
            ),
            reason(
                "depot.warehouse.ReprintLabel/reprinted",
                "mints `depot.warehouse.Label.barcode`"
            ),
        ]),
        "ESS draws a Bytes and a List from the counter it mints identities from, a List's draw \
         too although its value is `[]`, so the kernel draws both from `identity.mint`"
    );
    assert_eq!(
        requirements.get(&Capability::new(Capability::CLOCK)),
        None,
        "neither reads a clock"
    );
}
