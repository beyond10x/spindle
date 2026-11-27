//! Adversary cases of wave 30 U2 (`story:view-rows-absent-optional-fields`): an `Optional` view
//! field the row never wrote, read through the conformance target ([`HostTarget`]'s
//! `query_view`), against ESS's interpreter on the same steps, over the shapes the unit's own
//! fixture leaves out: an `Optional` list, an `Optional` struct and a struct's unwritten member, a
//! read through a reference into a row whose field is `omitted_when_absent`, an ordered view, a
//! filtered view with a parameter, presence declared on the view field itself (a computed field
//! and a related read among them), an aggregate grouped by an unwritten key and a paged view. A
//! served view ([`Host::listed_with`]) is read beside them to show it still leaves such a field
//! out.
//!
//! `tests/fixtures/adversary-w30-u2` keeps racks, each with an optional `kind`
//! (`omitted_when_absent`) and `tags`, and items, each with an optional `note`, `tags`, `cover`
//! (a struct whose `colour` is optional), `rank` and `rack_id`, a reference to a rack.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::entity::StateName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_host::{Host, SystemClock};

const FIXTURE: &str = "tests/fixtures/adversary-w30-u2";

/// One view read: its rows, in the order answered, and the total a paged view answers.
type Answer = (Vec<Map<String, Value>>, Option<u64>);

fn ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn object(value: &Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object: {value}");
    };
    object.clone()
}

/// Sends `commands` in order, each of which must take an outcome, and answers each read of
/// `reads` (a view and its parameters): one scenario on `target`.
fn answers<T: ConformanceTarget>(
    target: &T,
    commands: &[(&str, Value)],
    reads: &[(&str, Value)],
) -> Vec<Answer> {
    let context = ScenarioContext::new(
        "stack.items/authored/adversary-w30-u2"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w30-u2").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    for (command, input) in commands {
        target
            .execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: object(input)
                    .into_iter()
                    .map(|(field, value)| (field, node(value)))
                    .collect(),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
    }
    let answered = reads
        .iter()
        .map(|(view, params)| {
            let result = target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: object(params)
                        .into_iter()
                        .map(|(name, value)| (name, node(value)))
                        .collect::<BTreeMap<_, _>>(),
                    consistency: QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
                })
                .unwrap_or_else(|error| panic!("{view} reads: {error:?}"));
            let rows = result
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|(field, value)| {
                            (
                                field.clone(),
                                serde_json::to_value(value).expect("a node is JSON"),
                            )
                        })
                        .collect()
                })
                .collect();
            (rows, result.total)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    answered
}

/// The same steps on ESS's interpreter, then through the conformance target.
fn both(commands: &[(&str, Value)], reads: &[(&str, Value)]) -> (Vec<Answer>, Vec<Answer>) {
    let ir = ir();
    (
        answers(&Interpreted::for_model(ir.clone()), commands, reads),
        answers(&HostTarget::new(ir, laptop()), commands, reads),
    )
}

fn rack(input: Value) -> (&'static str, Value) {
    ("stack.items.AddRack", input)
}

fn item(input: Value) -> (&'static str, Value) {
    ("stack.items.AddItem", input)
}

/// Every read of `reads` after `commands` answers on the target what ESS's interpreter answers.
fn agrees(commands: &[(&str, Value)], reads: &[(&str, Value)]) {
    let (ess, spindle) = both(commands, reads);
    for (((view, params), ess), spindle) in reads.iter().zip(&ess).zip(&spindle) {
        assert_eq!(
            spindle, ess,
            "{view} with {params}: the target's answer (left) is ESS's interpreter's (right)"
        );
    }
}

#[test]
fn an_unwritten_optional_list_and_struct_read_as_ess_reads_them() {
    let (ess, _) = both(
        &[item(json!({"item_id": "i1"}))],
        &[("stack.items.Items", json!({}))],
    );
    assert_eq!(
        ess[0].0,
        [object(&json!({
            "item_id": "i1",
            "note": null,
            "tags": null,
            "cover": null,
            "rank": null,
            "rack_id": null,
            "rack_kind": null,
            "rack_tags": null,
            "rack_ref": null,
        }))],
        "ESS's interpreter answers every unwritten field of the row `null`"
    );
    agrees(
        &[item(json!({"item_id": "i1"}))],
        &[("stack.items.Items", json!({}))],
    );
}

#[test]
fn a_struct_written_without_its_optional_member_reads_as_ess_reads_it() {
    agrees(
        &[item(json!({"item_id": "i1", "cover": {"title": "plain"}}))],
        &[
            ("stack.items.Items", json!({})),
            ("stack.items.Except", json!({"skip": "none"})),
        ],
    );
}

#[test]
fn a_read_into_a_row_without_its_omitted_when_absent_field_reads_as_ess_reads_it() {
    agrees(
        &[
            rack(json!({"rack_id": "r1"})),
            rack(json!({"rack_id": "r2", "kind": "steel", "tags": ["top"]})),
            item(json!({"item_id": "i1", "rack_id": "r1"})),
            item(json!({"item_id": "i2", "rack_id": "r2"})),
            item(json!({"item_id": "i3", "rack_id": "r9"})),
        ],
        &[("stack.items.Items", json!({}))],
    );
}

#[test]
fn an_ordered_view_ranks_unwritten_fields_as_ess_ranks_them() {
    agrees(
        &[
            item(json!({"item_id": "i1", "rank": 2})),
            item(json!({"item_id": "i2"})),
            item(json!({"item_id": "i3", "rank": 5, "note": "top"})),
            item(json!({"item_id": "i4"})),
        ],
        &[("stack.items.Ranked", json!({}))],
    );
}

#[test]
fn a_filtered_view_with_a_parameter_reads_as_ess_reads_it() {
    agrees(
        &[
            item(json!({"item_id": "i1", "note": "kept"})),
            item(json!({"item_id": "i2"})),
            item(json!({"item_id": "i3", "cover": {"title": "t", "colour": "red"}})),
        ],
        &[
            ("stack.items.Except", json!({"skip": "i1"})),
            ("stack.items.Except", json!({"skip": "i2"})),
        ],
    );
}

/// A `shape:` view carries its struct's presence, not the source field's: `note` is
/// `omitted_when_absent` and `rank` `null_when_absent` on the row struct only.
#[test]
fn presence_declared_on_the_row_shape_reads_as_ess_reads_it() {
    agrees(
        &[
            item(json!({"item_id": "i1"})),
            item(json!({"item_id": "i2", "note": "n", "rank": 1, "tags": ["a"]})),
        ],
        &[("stack.items.Shaped", json!({}))],
    );
}

/// A field with a wire name other than its own, a computed field and a related read.
#[test]
fn renamed_and_computed_fields_read_as_ess_reads_them() {
    agrees(
        &[
            rack(json!({"rack_id": "r1"})),
            item(json!({"item_id": "i1"})),
            item(json!({"item_id": "i2", "rack_id": "r1"})),
            item(json!({"item_id": "i3", "note": "n"})),
        ],
        &[("stack.items.Renamed", json!({}))],
    );
}

#[test]
fn an_aggregate_grouped_by_an_unwritten_key_reads_as_ess_reads_it() {
    agrees(
        &[
            item(json!({"item_id": "i1", "note": "a"})),
            item(json!({"item_id": "i2"})),
            item(json!({"item_id": "i3"})),
        ],
        &[("stack.items.PerNote", json!({}))],
    );
}

#[test]
fn a_paged_view_reads_as_ess_reads_it() {
    agrees(
        &[
            item(json!({"item_id": "i1", "note": "a", "tags": []})),
            item(json!({"item_id": "i2"})),
            item(json!({"item_id": "i3", "tags": ["x"]})),
        ],
        &[
            ("stack.items.Paged", json!({})),
            ("stack.items.Paged", json!({"page": 1, "size": 2})),
        ],
    );
}

/// A rack established with its `omitted_when_absent` field held as `null`, read through the
/// item's reference: ESS's interpreter keeps the stored `null`, and the target answers `null`
/// for every related read.
#[test]
fn a_read_into_a_row_holding_null_reads_as_ess_reads_it() {
    let ir = ir();
    let read = |target: &dyn ConformanceTarget| {
        let context = ScenarioContext::new(
            "stack.items/authored/adversary-w30-u2-held"
                .parse()
                .expect("a scenario id"),
            CorrelationId::new("adversary-w30-u2-held").expect("a correlation id"),
        );
        target.begin_scenario(&context).expect("the scenario opens");
        target
            .establish_entity(EntitySetupRequest {
                entity: "stack.items.Rack".parse::<EntityRef>().expect("an entity"),
                identity: Node::Text("r1".to_owned()),
                fields: [
                    ("kind".to_owned(), Node::Null),
                    ("tags".to_owned(), Node::Null),
                ]
                .into_iter()
                .collect(),
                state: StateName::new("Standing").expect("a state"),
                correlation: context.correlation.clone(),
            })
            .expect("the rack is established");
        target
            .execute_command(SemanticCommandRequest {
                command: "stack.items.AddItem"
                    .parse::<CommandRef>()
                    .expect("a command"),
                actor: None,
                caller: None,
                input: [
                    ("item_id".to_owned(), Node::Text("i1".to_owned())),
                    ("rack_id".to_owned(), Node::Text("r1".to_owned())),
                ]
                .into_iter()
                .collect(),
                correlation: context.correlation.clone(),
            })
            .expect("the item is added");
        let rows = target
            .query_view(SemanticViewRequest {
                view: ViewRef::new("stack.items.Items".parse().expect("a view name")),
                params: BTreeMap::new(),
                consistency: QueryConsistency::Current,
                correlation: context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the view reads")
            .rows;
        target.end_scenario(&context).expect("the scenario closes");
        rows
    };
    let ess = read(&Interpreted::for_model(ir.clone()));
    let spindle = read(&HostTarget::new(ir, laptop()));
    assert_eq!(
        serde_json::to_value(&ess).expect("JSON")[0]["rack_kind"],
        Value::Null,
        "ESS's interpreter answers the held `null`: {ess:?}"
    );
    assert_eq!(spindle, ess, "the target answers the row ESS answers");
}

/// The served projection is the host's own and this unit leaves it alone: an unwritten field
/// without `null_when_absent` is left out, a related read through a reference naming no row too,
/// a field whose row shape declares `null_when_absent` is `null`, and a computed field is `null`.
#[test]
fn a_served_view_still_leaves_every_unwritten_field_out() {
    let mut host = Host::new(ir(), &laptop(), Arc::new(SystemClock), Path::new(""))
        .expect("the host admits the fixture");
    host.submit("stack.items.AddItem", object(&json!({"item_id": "i1"})))
        .expect("the item is added");
    assert_eq!(
        host.listed_with("stack.items.Items", &Map::new())
            .expect("the view reads")
            .rows,
        [object(&json!({"item_id": "i1"}))],
        "a served row leaves out every unwritten field"
    );
    assert_eq!(
        host.listed_with("stack.items.Shaped", &Map::new())
            .expect("the view reads")
            .rows,
        [object(&json!({"item_id": "i1", "rank": null}))],
        "a served row answers `null` under the shape's `null_when_absent`"
    );
    assert_eq!(
        host.listed_with("stack.items.Renamed", &Map::new())
            .expect("the view reads")
            .rows,
        [object(&json!({"item_id": "i1", "echo": null}))],
        "a served row answers a computed field `null`"
    );
}
