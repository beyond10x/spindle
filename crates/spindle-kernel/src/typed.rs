//! Whether a JSON value is a value of a declared type, as ESS's conformance interpreter decides it
//! (`input::validate_typed_value`).
//!
//! A primitive is decided by [`Primitive::admits`], ESS's one table for it: a canonical `Uuid`,
//! padded base64 `Bytes`, an integral number for an `Integer` (so `3.0` is the integer 3). A
//! newtype adds its prefix, its alphabet and its invariants, a struct its members and invariants,
//! an enum its variants, a union its tag. Every invariant must be true of the value; one the value
//! leaves unknown is refused too.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedField, ResolvedTypeRef};
use ess_domain::QualifiedName;
use ess_domain::entity::Invariant;
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};
use ess_primitives::facts::Number;
use ess_primitives::node::Node;
use ess_primitives::predicate::Truth;
use serde_json::{Map, Value};

use crate::facts::Facts;

/// Refuses `value` unless it is a value of `kind`, naming what it is not.
pub(crate) fn validate(ir: &EssIr, kind: &ResolvedTypeRef, value: &Value) -> Result<(), String> {
    check(ir, kind, value, 0)
}

fn check(ir: &EssIr, kind: &ResolvedTypeRef, value: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_TYPE_DEPTH {
        return Err(format!("the type nests deeper than {MAX_TYPE_DEPTH}"));
    }
    match kind {
        ResolvedTypeRef::Optional { of } => {
            if value.is_null() {
                Ok(())
            } else {
                check(ir, of, value, depth + 1)
            }
        }
        ResolvedTypeRef::Primitive { name } => {
            if primitive(*name, value) {
                Ok(())
            } else {
                Err(format!("{value} is not a {name}"))
            }
        }
        ResolvedTypeRef::List { of } => {
            let Value::Array(elements) = value else {
                return Err(format!("{value} is not a list"));
            };
            elements
                .iter()
                .enumerate()
                .try_for_each(|(index, element)| {
                    check(ir, of, element, depth + 1).map_err(|error| format!("[{index}]: {error}"))
                })
        }
        ResolvedTypeRef::Map { key, value: of } => {
            let Value::Object(entries) = value else {
                return Err(format!("{value} is not a map"));
            };
            entries.iter().try_for_each(|(spelling, entry)| {
                if !map_key(*key, spelling) {
                    return Err(format!("`{spelling}` is not a {key} map key"));
                }
                check(ir, of, entry, depth + 1).map_err(|error| format!("[{spelling}]: {error}"))
            })
        }
        ResolvedTypeRef::Declared { name } => {
            let declared = ir.named_type(name);
            body(ir, &declared.name, &declared.body, value, depth + 1)
        }
    }
}

fn body(
    ir: &EssIr,
    name: &QualifiedName,
    body: &ResolvedBody,
    value: &Value,
    depth: usize,
) -> Result<(), String> {
    match body {
        ResolvedBody::Newtype {
            of,
            alphabet,
            prefix,
            invariants,
        } => {
            check(ir, of, value, depth)?;
            if let (Some(prefix), Some(text)) = (prefix, value.as_str())
                && !text.starts_with(prefix.as_str())
            {
                return Err(format!(
                    "{text:?} does not start with {prefix:?}, as {name} does"
                ));
            }
            if let (Some(alphabet), Some(text)) = (alphabet, value.as_str())
                && let Some(outside) = text
                    .chars()
                    .find(|character| !alphabet.contains(*character))
            {
                return Err(format!(
                    "{outside:?} in {text:?} is not in the alphabet of {name}"
                ));
            }
            let wrapped = [ResolvedField {
                name: "value".to_owned(),
                type_ref: (*of).clone(),
                naming: ess_domain::name::Naming::default(),
            }];
            let mut values = Map::new();
            values.insert("value".to_owned(), value.clone());
            holds(ir, name, invariants, &wrapped, &values, value)
        }
        ResolvedBody::Struct { fields, invariants } => {
            let Value::Object(members) = value else {
                return Err(format!("{value} is not a {name}"));
            };
            let open = ir.undeclared_fields(name).is_ignored();
            let mut declared = Map::new();
            for (member, held) in members {
                if fields.iter().any(|field| field.name == *member) {
                    declared.insert(member.clone(), held.clone());
                } else if !open {
                    return Err(format!("{name} declares no member `{member}`"));
                }
            }
            for field in fields {
                match declared.get(&field.name) {
                    Some(held) => check(ir, &field.type_ref, held, depth + 1)
                        .map_err(|error| format!("`{}`: {error}", field.name))?,
                    None if field.type_ref.is_optional() => {}
                    None => return Err(format!("{name} requires `{}`", field.name)),
                }
            }
            holds(ir, name, invariants, fields, &declared, value)
        }
        ResolvedBody::Enum { variants } => {
            if value
                .as_str()
                .is_some_and(|text| variants.iter().any(|variant| variant == text))
            {
                Ok(())
            } else {
                Err(format!("{value} is not a variant of {name}"))
            }
        }
        ResolvedBody::Union { tag, variants } => {
            let Value::Object(members) = value else {
                return Err(format!("{value} is not a tagged {name}"));
            };
            let content = if tag == "value" { "content" } else { "value" };
            let selected = members
                .get(tag)
                .and_then(Value::as_str)
                .and_then(|label| variants.get(label))
                .ok_or_else(|| format!("{value} names no variant of {name} in `{tag}`"))?;
            let Some(selected) = selected else {
                return if members.len() == 1 {
                    Ok(())
                } else {
                    Err(format!("a unit variant of {name} holds its tag alone"))
                };
            };
            if members.len() != 2 {
                return Err(format!("a {name} holds exactly its tag and `{content}`"));
            }
            let payload = members
                .get(content)
                .ok_or_else(|| format!("a {name} carries its payload in `{content}`"))?;
            check(ir, selected, payload, depth)
        }
    }
}

/// Refuses unless every invariant is true of `values`, read against `fields`.
fn holds(
    ir: &EssIr,
    name: &QualifiedName,
    invariants: &[Invariant],
    fields: &[ResolvedField],
    values: &Map<String, Value>,
    value: &Value,
) -> Result<(), String> {
    if invariants.is_empty() {
        return Ok(());
    }
    let facts = Facts::values(ir, fields, values);
    for invariant in invariants {
        match invariant.predicate.evaluate(&facts) {
            Truth::True => {}
            truth => {
                return Err(format!(
                    "{value} is not a {name}: its invariant `{}` is {}",
                    invariant.statement,
                    truth.as_str()
                ));
            }
        }
    }
    Ok(())
}

/// Whether `value` is a value of `primitive`: ESS's [`Primitive::admits`], any finite number for a
/// `Binary64`, and anything at all for `Json`.
fn primitive(primitive: Primitive, value: &Value) -> bool {
    if primitive == Primitive::Json {
        return true;
    }
    let Ok(node) = serde_json::from_value::<Node>(value.clone()) else {
        return false;
    };
    match (primitive, &node) {
        (Primitive::Binary64, Node::Number(_)) => true,
        _ => primitive.admits(&node).is_some(),
    }
}

/// Whether `spelling` is a map key of `kind`, as ESS reads one: the literal spelling of a value,
/// and never a decimal one.
fn map_key(kind: Primitive, spelling: &str) -> bool {
    let value = match kind {
        Primitive::Boolean => match spelling {
            "true" => Node::Bool(true),
            "false" => Node::Bool(false),
            _ => return false,
        },
        Primitive::Integer => match spelling.parse::<i64>() {
            Ok(number) if number.to_string() == spelling => Node::Number(Number::from(number)),
            _ => return false,
        },
        Primitive::Decimal | Primitive::Binary64 | Primitive::Json => return false,
        Primitive::String
        | Primitive::Timestamp
        | Primitive::Duration
        | Primitive::Uuid
        | Primitive::Bytes => Node::Text(spelling.to_owned()),
    };
    kind.admits(&value).is_some()
}
