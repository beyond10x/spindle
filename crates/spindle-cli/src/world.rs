//! The world a served run's `POST /api/reset` may carry, read from its JSON body:
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
//! ([`spindle_host::Host::reset_to`]); a field present as `null` is held as absent. `decisions` is
//! a decision table in the decisions file's shape ([`Answers::from_json`]), laid over the run's own
//! answers until the next reset ([`Answers::over`]). `mint` seeds the identities and generated
//! values minted until the next reset ([`spindle_kernel::Seeded`]).

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use ess_compiler::EssIr;
use ess_compiler::refs::EntityRef;
use ess_domain::entity::StateName;
use ess_primitives::node::Node;
use serde_json::Value;
use spindle_host::{Seed, World};
use spindle_kernel::Row;

use crate::decisions::Answers;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default)]
    rows: Vec<SeedRow>,
    decisions: Option<Value>,
    mint: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedRow {
    entity: String,
    identity: Node,
    state: StateName,
    #[serde(default)]
    fields: BTreeMap<String, Node>,
}

/// Reads the world `body` carries against `ir`, its decision table laid over `run`.
///
/// # Errors
/// A body not of the shape above, a row ESS refuses, or a decision table [`Answers::from_json`]
/// refuses.
pub fn read(ir: &EssIr, body: &Value, run: &Answers) -> Result<World> {
    let body: Body = serde_json::from_value(body.clone()).context("reading the reset's world")?;
    let rows = body
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| seed(ir, row).with_context(|| format!("row {index}")))
        .collect::<Result<Vec<_>>>()?;
    let answers = body
        .decisions
        .map(|table| {
            Answers::from_json(ir, &table)
                .map_err(|error| anyhow!(error))
                .context("in the decision table")
        })
        .transpose()?
        .map(|table| table.over(run.clone()));
    Ok(World {
        rows,
        answers,
        mint: body.mint,
    })
}

/// `row`, once ESS admits it as the model declares its entity.
fn seed(ir: &EssIr, row: &SeedRow) -> Result<Seed> {
    let name = ir
        .entities()
        .keys()
        .find(|name| name.to_string() == row.entity)
        .ok_or_else(|| anyhow!("undeclared entity `{}`", row.entity))?;
    let identity = json(&row.identity)?;
    ess_conformance::input::validate_entity_setup(
        ir,
        &EntityRef::new(name.clone()),
        &row.identity,
        &row.fields,
        &row.state,
    )
    .map_err(|detail| anyhow!("{} {identity}: {detail}", row.entity))?;
    let fields = row
        .fields
        .iter()
        .filter(|(_, value)| !matches!(value, Node::Null))
        .map(|(field, value)| json(value).map(|value| (field.clone(), value)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Seed {
        entity: row.entity.clone(),
        identity,
        row: Row {
            state: row.state.as_str().to_owned(),
            fields,
        },
    })
}

fn json(node: &Node) -> Result<Value> {
    Ok(serde_json::to_value(node)?)
}
