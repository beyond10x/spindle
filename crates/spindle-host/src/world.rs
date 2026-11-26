//! The world a reset may carry, read from its JSON body:
//!
//! ```json
//! {"rows": [{"entity": "<entity>", "identity": <identity>, "state": "<State>",
//!            "fields": {"<field>": <value>}}],
//!  "decisions": {"<outcome>": true, "<command>": {"<outcome>": false},
//!                "rules": [{"command": "<command>", "outcome": "<outcome>", "answer": true,
//!                           "when": {"input.<field>": <value>}}]},
//!  "mint": 7}
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
//! values minted until the next reset ([`spindle_kernel::Seeded`]).
//!
//! `run`'s served reset reads its world here, and so does every wasm host that loads one.

use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
use ess_compiler::refs::EntityRef;
use ess_domain::entity::StateName;
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};
use ess_primitives::facts::Number;
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

/// A row as the body spells it. ESS checks its identity and fields as the [`Node`]s it reads them
/// as; a `Node` is not written back, since ESS writes an integral number as a float (`1` as
/// `1.0`). The row is held as written with its `Integer` values as integers ([`integers`]).
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
    let held = integers(ir, &entity.identity.type_ref, identity, 0)
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
                Some(declared) => integers(ir, &declared.type_ref, value, 0)
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

/// `value`, of `kind`, with every value the model types `Integer` written as the JSON integer
/// (`1.0` as `1`), as the kernel writes one (`spindle_kernel::compute`): through `Optional`,
/// newtypes, `List` elements, `Map` values, struct members and a union's payload. Everything else
/// is held as written. ESS has checked `value` against `kind` before.
///
/// # Errors
/// An `Integer` that is not an integral number within `i64`.
fn integers(
    ir: &EssIr,
    kind: &ResolvedTypeRef,
    value: &Value,
    depth: usize,
) -> Result<Value, String> {
    if depth > MAX_TYPE_DEPTH {
        return Err(format!("the type nests deeper than {MAX_TYPE_DEPTH}"));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    if ir.literal_primitive(kind) == Some(Primitive::Integer) {
        return serde_json::from_value::<Number>(value.clone())
            .ok()
            .and_then(Number::as_i64)
            .map(Value::from)
            .ok_or_else(|| format!("{value} is not an Integer within 64 bits"));
    }
    let nested = |of: &ResolvedTypeRef, value: &Value| integers(ir, of, value, depth + 1);
    match (kind.required(), value) {
        (ResolvedTypeRef::List { of }, Value::Array(elements)) => elements
            .iter()
            .enumerate()
            .map(|(index, element)| nested(of, element).map_err(|why| format!("[{index}]: {why}")))
            .collect(),
        (ResolvedTypeRef::Map { value: of, .. }, Value::Object(entries)) => entries
            .iter()
            .map(|(key, entry)| {
                nested(of, entry)
                    .map(|entry| (key.clone(), entry))
                    .map_err(|why| format!("{key:?}: {why}"))
            })
            .collect(),
        (ResolvedTypeRef::Declared { name }, _) => match &ir.named_type(name).body {
            ResolvedBody::Newtype { of, .. } => nested(of, value),
            ResolvedBody::Struct { fields, .. } => match value {
                Value::Object(members) => members
                    .iter()
                    .map(|(member, held)| {
                        let held = match fields.iter().find(|field| field.name == *member) {
                            Some(field) => nested(&field.type_ref, held)
                                .map_err(|why| format!("`{member}`: {why}"))?,
                            None => held.clone(),
                        };
                        Ok((member.clone(), held))
                    })
                    .collect(),
                _ => Ok(value.clone()),
            },
            ResolvedBody::Union { tag, variants } => {
                let content = if tag == "value" { "content" } else { "value" };
                let selected = value
                    .get(tag)
                    .and_then(Value::as_str)
                    .and_then(|label| variants.get(label))
                    .and_then(Option::as_ref);
                match (selected, value) {
                    (Some(selected), Value::Object(members)) => members
                        .iter()
                        .map(|(member, held)| {
                            let held = if member == content {
                                nested(selected, held)?
                            } else {
                                held.clone()
                            };
                            Ok((member.clone(), held))
                        })
                        .collect(),
                    _ => Ok(value.clone()),
                }
            }
            ResolvedBody::Enum { .. } => Ok(value.clone()),
        },
        _ => Ok(value.clone()),
    }
}

/// `value` as ESS reads it.
fn node(value: &Value) -> Result<Node, String> {
    <Node as serde::Deserialize>::deserialize(value).map_err(|error| error.to_string())
}
