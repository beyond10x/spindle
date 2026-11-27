//! The world a reset may carry, read from its JSON body:
//!
//! ```json
//! {"rows": [{"entity": "<entity>", "identity": <identity>, "state": "<State>",
//!            "fields": {"<field>": <value>}}],
//!  "decisions": {"<outcome>": true, "<command>": {"<outcome>": false},
//!                "rules": [{"command": "<command>", "outcome": "<outcome>", "answer": true,
//!                           "when": {"input.<field>": <value>}}]},
//!  "mint": 7,
//!  "clock": "2026-07-01T00:00:00Z"}
//! ```
//!
//! Every key is optional and no other is read. Each row is checked against the model by ESS as an
//! `establish` step is ([`ess_conformance::input::validate_entity_setup`]), then by the host
//! ([`crate::Host::reset_to`]); a field present as `null` is held as absent. Every value the model
//! types `Integer` (through `Optional`, newtypes, lists, maps, struct members and union payloads)
//! is held as the integer, as `{increment: ...}` and an expression write one, so `1.0` and `1`
//! name one value; every other value is held as written. `decisions` is a
//! decision table in the decisions file's shape ([`Answers::from_json`]), laid over the run's own
//! answers until the next reset ([`Answers::over`]). `mint` seeds the identities and generated
//! values minted until the next reset ([`spindle_kernel::Seeded`]). `clock`, an RFC 3339
//! `date-time` ([`crate::instant`]), moves the host's stepping clock forward to that instant
//! ([`World::clock`]); a host without one refuses the world, and so does one whose clock stands
//! after it, before anything is emptied ([`crate::Host::reset_to`]).
//!
//! `run`'s served reset reads its world here, and so does every wasm host that loads one.

use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_compiler::refs::EntityRef;
use ess_domain::entity::StateName;
use ess_primitives::node::Node;
use serde_json::Value;
use spindle_kernel::{Decider, Row, integers};

use crate::{Answers, Seed, World};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    #[serde(default)]
    rows: Vec<SeedRow>,
    decisions: Option<Value>,
    mint: Option<u64>,
    // A default, so a world spelled as an array of the three keys before it is read as before.
    #[serde(default)]
    clock: Option<String>,
}

/// A row as the body spells it. ESS checks its identity and fields as the [`Node`]s it reads them
/// as; a `Node` is not written back, since ESS writes an integral number as a float (`1` as
/// `1.0`). The row is held as written with its `Integer` values as integers
/// ([`spindle_kernel::integers`]).
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
/// A body not of the shape above, a row ESS refuses, a decision table [`Answers::from_json`]
/// refuses, or a `clock` that is no RFC 3339 instant; the reason names which, as
/// `reading the reset's world: ...`, `row <n>: ...`, `in the decision table: ...` or
/// `clock: ...`.
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
    let clock = body
        .clock
        .map(|text| {
            crate::instant(&text).ok_or_else(|| {
                "clock: expected an RFC 3339 instant such as 2026-02-01T00:00:00Z".to_owned()
            })
        })
        .transpose()?;
    Ok(World {
        rows,
        answers,
        mint: body.mint,
        clock,
    })
}

/// `row`, once ESS admits it as the model declares its entity.
fn seed(ir: &EssIr, row: &SeedRow) -> Result<Seed, String> {
    let (name, entity) = ir
        .entities()
        .iter()
        .find(|(name, _)| name.to_string() == row.entity)
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
    let refused = |what: &str, why: String| format!("{} {identity}: {what}: {why}", row.entity);
    // The identity in the one spelling a command's is keyed in, so a command finds the row by
    // `1` or `1.0` of a `Decimal` identity (`spindle_kernel::one_spelling`).
    let held = integers(ir, &entity.identity.type_ref, identity)
        .map(|held| spindle_kernel::one_spelling(&held))
        .map_err(|why| refused("the identity", why))?;
    let fields = row
        .fields
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(field, value)| {
            let held = match entity
                .fields
                .iter()
                .find(|declared| declared.name == *field)
            {
                Some(declared) => integers(ir, &declared.type_ref, value)
                    .map_err(|why| refused(&format!("`{field}`"), why))?,
                None => value.clone(),
            };
            Ok((field.clone(), held))
        })
        .collect::<Result<_, String>>()?;
    Ok(Seed {
        entity: row.entity.clone(),
        identity: held,
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
