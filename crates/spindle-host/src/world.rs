//! The world a reset may carry, read from its JSON body:
//!
//! ```json
//! {"rows": [{"entity": "<entity>", "identity": <identity>, "state": "<State>",
//!            "fields": {"<field>": <value>}}],
//!  "decisions": {"<outcome>": true, "<command>": {"<outcome>": false}},
//!  "mint": 7}
//! ```
//!
//! Every key is optional and no other is read. Each row is checked against the model by ESS as an
//! `establish` step is ([`ess_conformance::input::validate_entity_setup`]), then by the host
//! ([`crate::Host::reset_to`]); a field present as `null` is held as absent. `decisions` is a
//! decision table in the decisions file's shape ([`Answers::from_json`]), laid over the run's own
//! answers until the next reset ([`Answers::over`]). `mint` seeds the identities and generated
//! values minted until the next reset ([`spindle_kernel::Seeded`]).
//!
//! `run`'s served reset reads its world here, and so does every wasm host that loads one.

use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_compiler::refs::EntityRef;
use ess_domain::entity::StateName;
use ess_primitives::node::Node;
use serde_json::Value;
use spindle_kernel::{Decider, Row};

use crate::{Answers, Seed, World};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default)]
    rows: Vec<SeedRow>,
    decisions: Option<Value>,
    mint: Option<u64>,
}

/// A row as the body spells it. Its identity and fields are held as written, as a command's input
/// is, and ESS checks them as the [`Node`]s it reads them as; a `Node` is not written back, since
/// ESS writes an integral number as a float (`1` as `1.0`).
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedRow {
    entity: String,
    identity: Value,
    state: StateName,
    #[serde(default)]
    fields: BTreeMap<String, Value>,
}

/// Reads the world `body` carries against `ir`, its decision table made the decider `over` makes
/// of it (laid over the run's answers).
///
/// # Errors
/// A body not of the shape above, a row ESS refuses, or a decision table [`Answers::from_json`]
/// refuses; the reason names which, as `reading the reset's world: ...`, `row <n>: ...` or
/// `in the decision table: ...`.
pub fn read_world(
    ir: &EssIr,
    body: &Value,
    over: impl FnOnce(Answers) -> Box<dyn Decider + Send>,
) -> Result<World, String> {
    let body: Body = serde_json::from_value(body.clone())
        .map_err(|error| format!("reading the reset's world: {error}"))?;
    let rows = body
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| seed(ir, row).map_err(|why| format!("row {index}: {why}")))
        .collect::<Result<Vec<_>, _>>()?;
    let answers = body
        .decisions
        .map(|table| {
            Answers::from_json(ir, &table)
                .map_err(|error| format!("in the decision table: {error}"))
        })
        .transpose()?
        .map(over);
    Ok(World {
        rows,
        answers,
        mint: body.mint,
    })
}

/// `row`, once ESS admits it as the model declares its entity.
fn seed(ir: &EssIr, row: &SeedRow) -> Result<Seed, String> {
    let name = ir
        .entities()
        .keys()
        .find(|name| name.to_string() == row.entity)
        .ok_or_else(|| format!("undeclared entity `{}`", row.entity))?;
    let identity = &row.identity;
    let nodes = row
        .fields
        .iter()
        .map(|(field, value)| node(value).map(|value| (field.clone(), value)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    ess_conformance::input::validate_entity_setup(
        ir,
        &EntityRef::new(name.clone()),
        &node(identity)?,
        &nodes,
        &row.state,
    )
    .map_err(|detail| format!("{} {identity}: {detail}", row.entity))?;
    let fields = row
        .fields
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect();
    Ok(Seed {
        entity: row.entity.clone(),
        identity: identity.clone(),
        row: Row {
            state: row.state.as_str().to_owned(),
            fields,
        },
    })
}

/// `value` as ESS reads it.
fn node(value: &Value) -> Result<Node, String> {
    <Node as serde::Deserialize>::deserialize(value).map_err(|error| error.to_string())
}
