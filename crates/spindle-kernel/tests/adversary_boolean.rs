//! Adversary cases for `story:generated-boolean-values`, over the neutral fixture
//! `tests/fixtures/adversary_boolean`: each case runs one command sequence on ESS 0.86.0's own
//! interpreter and on the kernel with a mint that answers ESS's tick UUIDs, and compares the
//! minted Booleans.
//!
//! The claim under attack (`crates/spindle-kernel/src/mint.rs`, module documentation): "a
//! [`Mint`] that answers those UUIDs mints the Booleans the interpreter does". ESS draws one tick
//! per minted value in the order `interpret::execute::take` walks an outcome: every published
//! value a `{payload: <Event>.<field>}` in `sets:` reads first (`publish_reads`), then a creation
//! identity (`create`), then the remaining `sets:`, then the payload (`emit`).

use std::collections::BTreeMap;
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
use spindle_kernel::{Executed, Identity, MemoryStore, NoExternals, Store, Uuid, execute_minting};

const FLAG: &str = "semaphore.flag.Flag";
const FLAG_ID: &str = "7a2e4c1d-5b3f-4e6a-8c9d-1f2e3d4c5b6a";

fn flags() -> spindle_spec::EssIr {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_boolean");
    spindle_spec::load(&root)
        .expect("the adversary Boolean fixture compiles")
        .ir
}

/// Payloads or view rows, each by field name.
type Rows = Vec<Map<String, Value>>;

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// What ESS's interpreter publishes for each command in turn (the one direct event's payload),
/// and the `FlagCard` rows after the last.
fn interpreted(ir: &spindle_spec::EssIr, commands: &[(&str, (&str, &str))]) -> (Rows, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "semaphore.flag/authored/adversary-boolean"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-boolean").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut published = Vec::new();
    for (command, (name, sent)) in commands {
        let executed = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input: BTreeMap::from([((*name).to_owned(), Node::Text((*sent).to_owned()))]),
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
            view: ViewRef::new("semaphore.flag.FlagCard".parse().expect("a view name")),
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

/// A mint answering the UUID ESS's interpreter mints on tick 1, 2, … in turn, as
/// `tests/generated.rs`'s `Ticks` does.
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

/// What the kernel publishes for each command in turn, the draws it made, and the stored row
/// of the flag the last event names.
fn spindled(
    ir: &spindle_spec::EssIr,
    commands: &[(&str, (&str, &str))],
) -> (Rows, Vec<String>, Map<String, Value>) {
    let mut store = MemoryStore::default();
    let mut mint = Ticks {
        ir: ir.clone(),
        tick: 0,
        asked: Vec::new(),
    };
    let mut published = Vec::new();
    for (command, (name, sent)) in commands {
        let mut input = Map::new();
        input.insert((*name).to_owned(), json!(sent));
        let executed: Executed =
            execute_minting(ir, &mut store, &mut NoExternals, &mut mint, command, &input)
                .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
        assert_eq!(executed.events.len(), 1, "{command}: {:?}", executed.events);
        published.push(executed.events[0].payload.clone());
    }
    let identity = published
        .last()
        .and_then(|payload| payload.get("flag_id"))
        .cloned()
        .expect("the last event names its flag");
    let row = store
        .load(FLAG, &identity)
        .expect("the store reads")
        .expect("the flag is stored")
        .row
        .fields
        .into_iter()
        .collect();
    (published, mint.asked, row)
}

/// ESS draws the `up` a creation's `sets:` copies from its event before it mints the creation's
/// identity (`take` calls `publish_reads` before `create`): `up` is tick 1, `false`, and the
/// identity tick 2. The kernel mints the identity first (`Run::take`, `mint_identity` before
/// `write`), so `up` is its tick 2, `true`.
#[test]
fn a_published_boolean_a_creation_stores_is_drawn_before_the_minted_identity_as_ess_draws_it() {
    let ir = flags();
    let commands = [("semaphore.flag.RaiseFlag", ("note", "n"))];
    let (ess_published, ess_rows) = interpreted(&ir, &commands);
    assert_eq!(
        ess_published[0]["up"],
        json!(false),
        "ESS mints `up` on tick 1"
    );
    assert_eq!(
        ess_published[0]["flag_id"],
        json!("00000000-0000-4000-8000-000000000002"),
        "ESS mints the identity on tick 2"
    );
    assert_eq!(ess_rows.len(), 1, "{ess_rows:?}");

    let (published, asked, row) = spindled(&ir, &commands);
    assert_eq!(
        (published[0]["up"].clone(), row["up"].clone(), asked),
        (
            ess_published[0]["up"].clone(),
            ess_rows[0]["up"].clone(),
            vec![
                "semaphore.flag.FlagRaised.up".to_owned(),
                "semaphore.flag.Flag.flag_id".to_owned()
            ]
        ),
        "the published and stored `up` are ESS's, drawn in ESS's order"
    );
}

/// On an update, ESS draws the published `signal` that `sets:` copies before the `waved` that
/// `sets:` lists first: after the plant's `up` (tick 1), `signal` is tick 2 (`true`) and `waved`
/// tick 3 (`false`). The kernel walks `sets:` in order and draws `waved` on tick 2.
#[test]
fn a_published_boolean_an_update_stores_is_drawn_before_the_other_sets_as_ess_draws_it() {
    let ir = flags();
    let commands = [
        ("semaphore.flag.PlantFlag", ("flag_id", FLAG_ID)),
        ("semaphore.flag.WaveFlag", ("flag_id", FLAG_ID)),
    ];
    let (ess_published, ess_rows) = interpreted(&ir, &commands);
    assert_eq!(ess_rows.len(), 1, "{ess_rows:?}");
    assert_eq!(ess_published[1]["signal"], json!(true), "ESS: tick 2");
    assert_eq!(ess_rows[0]["waved"], json!(false), "ESS: tick 3");

    let (published, _, row) = spindled(&ir, &commands);
    assert_eq!(
        (
            published[1]["signal"].clone(),
            row["signal"].clone(),
            row["waved"].clone(),
            row["up"].clone()
        ),
        (
            ess_published[1]["signal"].clone(),
            ess_rows[0]["signal"].clone(),
            ess_rows[0]["waved"].clone(),
            ess_rows[0]["up"].clone()
        ),
        "the published `signal` and stored `waved` are ESS's"
    );
}

/// Two generated Boolean members of one published struct, drawn in member order by both.
#[test]
fn generated_boolean_struct_members_are_minted_as_ess_mints_them() {
    let ir = flags();
    let commands = [
        ("semaphore.flag.PlantFlag", ("flag_id", FLAG_ID)),
        ("semaphore.flag.SignalFlag", ("flag_id", FLAG_ID)),
    ];
    let (ess_published, ess_rows) = interpreted(&ir, &commands);
    let (published, _, row) = spindled(&ir, &commands);
    assert_eq!(published[1]["pair"], ess_published[1]["pair"]);
    assert_eq!(row["up"], ess_rows[0]["up"]);
}
