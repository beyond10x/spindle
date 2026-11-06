//! Adversary cases for `story:generated-bytes-and-list-values`, over the neutral fixture
//! `tests/fixtures/adversary_bytes`: a bin whose creation mints a `Bytes`, a present
//! `Optional<Bytes>`, an absent `Optional<Bytes>`, a newtype over `Bytes`, a `List` of records,
//! a newtype over a `List`, a present `Optional<List<Bytes>>` and a second `Bytes` in `sets:`,
//! a published `Bytes` the row stores, a struct of two `Bytes` and an event field declared before
//! the payload writes it.
//!
//! The claim under attack (`crates/spindle-kernel/src/mint.rs`, module documentation): a
//! `Bytes` is "the one byte ESS mints on that tick", read back from the UUID's last group, and a
//! `List` is `[]` and "still takes its draw". A `Bytes` makes the draw order visible exactly (the
//! byte is the tick), where a `Boolean` showed only its parity.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
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
    Executed, Identity, MemoryStore, NoExternals, Store, Uuid, execute, execute_minting,
};

const BIN: &str = "hold.bin.Bin";
const BIN_ID: &str = "3c5e7a9b-2d4f-4a6b-8c1d-5e7f9a1b3c5d";

fn bins() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_bytes");
    spindle_spec::load(&root)
        .expect("the adversary Bytes fixture compiles")
        .ir
}

type Rows = Vec<Map<String, Value>>;

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn input_of(sent: &Value) -> Map<String, Value> {
    sent.as_object().expect("an input is an object").clone()
}

fn nodes_of(sent: &Value) -> BTreeMap<String, Node> {
    input_of(sent)
        .into_iter()
        .map(|(name, value)| {
            let Value::String(text) = value else {
                panic!("the cases send text only")
            };
            (name, Node::Text(text))
        })
        .collect()
}

/// The one direct event's payload ESS's interpreter publishes for each command in turn, and the
/// `BinCard` rows after the last.
fn interpreted(ir: &spindle_spec::EssIr, commands: &[(&str, Value)]) -> (Rows, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "hold.bin/authored/adversary-bytes"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-bytes").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut published = Vec::new();
    for (command, sent) in commands {
        let executed = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: nodes_of(sent),
                correlation: context.correlation.clone(),
            })
            .expect("the interpreter runs the command");
        assert_eq!(executed.direct_events.len(), 1, "{command}: {executed:?}");
        published.push(
            executed.direct_events[0]
                .payload
                .iter()
                .map(|(field, node)| (field.clone(), json_of(node)))
                .collect(),
        );
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new("hold.bin.BinCard".parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            row.iter()
                .map(|(field, node)| (field.clone(), json_of(node)))
                .collect()
        })
        .collect();
    (published, rows)
}

/// A mint answering the UUID ESS's interpreter mints on tick `tick + 1`, `tick + 2`, … in turn.
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

/// What the kernel publishes for each command in turn, drawing from `mint`, and the stored row.
fn spindled(
    ir: &spindle_spec::EssIr,
    mint: &mut Ticks,
    commands: &[(&str, Value)],
) -> (Rows, Map<String, Value>) {
    let mut store = MemoryStore::default();
    let mut published = Vec::new();
    for (command, sent) in commands {
        let executed: Executed = execute_minting(
            ir,
            &mut store,
            &mut NoExternals,
            mint,
            command,
            &input_of(sent),
        )
        .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
        assert_eq!(executed.events.len(), 1, "{command}: {:?}", executed.events);
        published.push(executed.events[0].payload.clone());
    }
    let row = store
        .load(BIN, &json!(BIN_ID))
        .expect("the store reads")
        .expect("the bin is stored")
        .row
        .fields
        .into_iter()
        .collect();
    (published, row)
}

/// The row as a view would show it: every stored field, without the identity's absence.
fn without_identity(row: &Map<String, Value>) -> Map<String, Value> {
    row.iter()
        .filter(|(field, _)| field.as_str() != "bin_id")
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect()
}

fn fill() -> (&'static str, Value) {
    ("hold.bin.FillBin", json!({"bin_id": BIN_ID}))
}

/// Every Bytes and List a creation mints, in one outcome, is the value ESS's interpreter mints:
/// the published `code` the row stores first (`publish_reads`), then `sets:` in the order
/// written (the absent `skipped` takes no tick), then the event's fields in its declared order.
#[test]
fn several_generated_bytes_and_lists_in_one_creation_are_minted_as_ess_mints_them() {
    let ir = bins();
    let commands = [fill()];
    let (ess_published, ess_rows) = interpreted(&ir, &commands);
    assert_eq!(ess_rows.len(), 1, "{ess_rows:?}");

    let mut mint = Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    };
    let (published, row) = spindled(&ir, &mut mint, &commands);
    assert_eq!(
        published[0], ess_published[0],
        "the published payload is ESS's (draws: {:?})",
        mint.asked
    );
    let ess_row: Map<String, Value> = ess_rows[0]
        .iter()
        .filter(|(field, value)| field.as_str() != "bin_id" && !value.is_null())
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect();
    assert_eq!(
        without_identity(&row),
        ess_row,
        "the stored row is ESS's (draws: {:?})",
        mint.asked
    );
}

/// An update's `{input: raw, else: {generated: true}}` with no `raw` sent, then a second minted
/// `Bytes`, after a creation's draws.
#[test]
fn a_refill_mints_its_bytes_as_ess_mints_them() {
    let ir = bins();
    let commands = [fill(), ("hold.bin.RefillBin", json!({"bin_id": BIN_ID}))];
    let (_, ess_rows) = interpreted(&ir, &commands);
    assert_eq!(ess_rows.len(), 1, "{ess_rows:?}");
    let mut mint = Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    };
    let (_, row) = spindled(&ir, &mut mint, &commands);
    assert_eq!(
        (row["raw"].clone(), row["tail"].clone()),
        (ess_rows[0]["raw"].clone(), ess_rows[0]["tail"].clone()),
        "the refilled raw and tail are ESS's"
    );
}

/// The `Bytes` ESS's interpreter mints on `tick`, through its public `values::mint`.
fn ess_bytes(ir: &spindle_spec::EssIr, tick: u64) -> Value {
    let bytes = ess_compiler::ir::ResolvedTypeRef::Primitive {
        name: ess_domain::types::Primitive::Bytes,
    };
    match ess_conformance::interpret::values::mint(ir, &bytes, tick) {
        Ok(Some(node)) => json_of(&node),
        other => panic!("ESS mints a Bytes on tick {tick}: {other:?}"),
    }
}

/// Past tick 255 the low byte wraps; past `u32::MAX` ESS's ordinal saturates. The kernel reads
/// the tick back from the UUID's last group: on each counter start, every `Bytes` `sets:` writes
/// is the one ESS mints on the tick the kernel drew it on.
#[test]
fn bytes_minted_past_tick_255_and_past_u32_max_are_ess_bytes() {
    let ir = bins();
    let starts = [
        250,
        u64::from(u32::MAX) - 6,
        u64::from(u32::MAX) + 300,
        ess_conformance::interpret::values::MAX_UUID_TICK - 20,
    ];
    for start in starts {
        let mut mint = Ticks {
            ir: ir.clone(),
            tick: start,
            asked: Vec::new(),
        };
        let (_, row) = spindled(&ir, &mut mint, &[fill()]);
        for field in ["raw", "spare", "blob", "tail", "code"] {
            let drawn = mint
                .asked
                .iter()
                .position(|asked| asked.ends_with(&format!(".{field}")))
                .unwrap_or_else(|| panic!("`{field}` is drawn: {:?}", mint.asked));
            let tick = start + u64::try_from(drawn).expect("a small index") + 1;
            assert_eq!(
                row[field],
                ess_bytes(&ir, tick),
                "`{field}` drawn on tick {tick} (counter started at {start})"
            );
        }
    }
}

/// The production mint, `uuid-v4` (`execute`, `UuidV4`): its UUIDs are not ESS's tick UUIDs, and
/// the module documentation says such a UUID still "gives a spread of bytes". Each of 32
/// creations mints its `raw` from a fresh random UUID; a byte drawn uniformly would give about
/// 30 distinct values, and fewer than 8 has odds far below one in a billion.
#[test]
fn bytes_minted_from_random_uuids_spread() {
    let ir = bins();
    let mut seen = BTreeSet::new();
    let mut all = Vec::new();
    for n in 0..32_u32 {
        let mut store = MemoryStore::default();
        let id = format!("3c5e7a9b-2d4f-4a6b-8c1d-{n:012}");
        execute(
            &ir,
            &mut store,
            &mut NoExternals,
            "hold.bin.FillBin",
            &input_of(&json!({"bin_id": id})),
        )
        .expect("a bin fills");
        let raw = store
            .load(BIN, &json!(id))
            .expect("the store reads")
            .expect("the bin is stored")
            .row
            .fields["raw"]
            .clone();
        let text = raw.as_str().expect("a Bytes is text").to_owned();
        assert!(
            text.len() == 4 && text.ends_with("=="),
            "a one-byte padded base64 value: {text}"
        );
        seen.insert(text.clone());
        all.push(text);
    }
    assert!(
        seen.len() >= 8,
        "32 random mints give {} distinct Bytes: {all:?}",
        seen.len()
    );
}

/// The two random barcodes the story's `two_mints_differ` speaks of, on the production mint: two
/// creations, two `Bytes`, which must differ.
#[test]
fn two_bytes_mints_differ_on_the_production_mint() {
    let ir = bins();
    let mut store = MemoryStore::default();
    let first = "3c5e7a9b-2d4f-4a6b-8c1d-000000000001";
    let second = "3c5e7a9b-2d4f-4a6b-8c1d-000000000002";
    let mut raw = Vec::new();
    for id in [first, second] {
        execute(
            &ir,
            &mut store,
            &mut NoExternals,
            "hold.bin.FillBin",
            &input_of(&json!({"bin_id": id})),
        )
        .expect("a bin fills");
        let row = store
            .load(BIN, &json!(id))
            .expect("the store reads")
            .expect("the bin is stored")
            .row
            .fields;
        raw.push((row["raw"].clone(), row["tail"].clone(), row["code"].clone()));
    }
    assert_ne!(
        raw[0], raw[1],
        "two creations on the production mint mint different Bytes"
    );
}

/// Derivation names `identity.mint` for each minted Bytes and List, a present `Optional` among
/// them, and nothing for the absent `skipped`.
#[test]
fn derivation_names_identity_mint_for_each_bytes_and_list_and_not_for_an_absent_one() {
    use spindle_capability::{Capability, derive};

    let ir = bins();
    let requirements = derive(&ir, Some("keeper")).expect("keeper is a component");
    let because: Vec<String> = requirements
        .get(&Capability::new(Capability::IDENTITY_MINT))
        .map(|reasons| {
            reasons
                .iter()
                .map(|reason| reason.because.clone())
                .collect()
        })
        .unwrap_or_default();
    for field in [
        "hold.bin.Bin.raw",
        "hold.bin.Bin.spare",
        "hold.bin.Bin.blob",
        "hold.bin.Bin.parts",
        "hold.bin.Bin.shelf",
        "hold.bin.Bin.chunks",
        "hold.bin.Bin.tail",
        "hold.bin.BinFilled.code",
        "hold.bin.BinFilled.extra",
    ] {
        assert!(
            because.contains(&format!("mints `{field}`")),
            "`{field}` is named: {because:?}"
        );
    }
    assert!(
        !because.iter().any(|reason| reason.contains("skipped")),
        "an absent Optional mints nothing: {because:?}"
    );
}
