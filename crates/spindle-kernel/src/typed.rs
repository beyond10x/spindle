//! Whether a JSON value is a value of a declared type, as ESS's conformance interpreter decides it
//! (`input::validate_typed_value`).
//!
//! A primitive is decided by [`Primitive::admits`], ESS's one table for it: a canonical `Uuid`,
//! padded base64 `Bytes`, an integral number for an `Integer` (so `3.0` is the integer 3). A
//! newtype adds its prefix, its alphabet and its invariants, a struct its members and invariants,
//! an enum its variants, a union its tag. Every invariant must be true of the value; one the value
//! leaves unknown is refused too.

use std::borrow::Cow;

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

/// `value`, of `kind`, with every value the model types `Integer` written as the JSON integer
/// (`1.0` as `1`), as the kernel writes one ([`crate::compute`]): through `Optional`, newtypes,
/// `List` elements, `Map` values, struct members and a union's payload. Everything else is held as
/// written. `value` is checked against `kind` before ([`validate`]), so `1` and `1.0` are one value
/// as ESS reads them (`Number` compares the exact value) and are written alike.
///
/// # Errors
/// An `Integer` that is not an integral number within `i64`: one ESS does not admit
/// (`Number::is_integral` is `as_i64`), so none reaches here from an admitted value.
pub(crate) fn integers(ir: &EssIr, kind: &ResolvedTypeRef, value: &Value) -> Result<Value, String> {
    integral(ir, kind, value, 0)
}

fn integral(
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
    let nested = |of: &ResolvedTypeRef, value: &Value| integral(ir, of, value, depth + 1);
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

/// `identity` with every number in one spelling, whatever type the model declares it: an integral
/// number within `i64` or `u64` as that integer (`1.0`, `1e0` and `-0.0` as `1`, `1` and `0`),
/// any other as the binary64 `serde_json` reads (`5e-1` as `0.5`). ESS keys an instance by the
/// value it reads and compares numbers by their exact value whatever their declared type
/// (`Number`), so the spellings of one number name one instance; a store keys an instance by its
/// identity's JSON text, so the kernel hands it every identity so. Borrowed where nothing changes.
pub(crate) fn spelled(identity: &Value) -> Cow<'_, Value> {
    match identity {
        Value::Number(number) => match one_number(number) {
            Some(number) => Cow::Owned(Value::Number(number)),
            None => Cow::Borrowed(identity),
        },
        Value::Array(elements) => {
            let respelled: Vec<Cow<'_, Value>> = elements.iter().map(spelled).collect();
            if respelled
                .iter()
                .all(|element| matches!(element, Cow::Borrowed(_)))
            {
                Cow::Borrowed(identity)
            } else {
                Cow::Owned(Value::Array(
                    respelled.into_iter().map(Cow::into_owned).collect(),
                ))
            }
        }
        Value::Object(members) => {
            let respelled: Vec<(&String, Cow<'_, Value>)> = members
                .iter()
                .map(|(name, member)| (name, spelled(member)))
                .collect();
            if respelled
                .iter()
                .all(|(_, member)| matches!(member, Cow::Borrowed(_)))
            {
                Cow::Borrowed(identity)
            } else {
                Cow::Owned(Value::Object(
                    respelled
                        .into_iter()
                        .map(|(name, member)| (name.clone(), member.into_owned()))
                        .collect(),
                ))
            }
        }
        Value::Null | Value::Bool(_) | Value::String(_) => Cow::Borrowed(identity),
    }
}

/// `number` in its one spelling where that differs from how it is held: a binary64 that is an
/// integer within `i64` or `u64` as that integer, negative zero as `0`. `None` for an integer
/// already, and for any other binary64, which is its own one spelling.
fn one_number(number: &serde_json::Number) -> Option<serde_json::Number> {
    /// 2^63 and 2^64, exact as binary64.
    const I64_END: f64 = 9_223_372_036_854_775_808.0;
    const U64_END: f64 = 18_446_744_073_709_551_616.0;
    if number.is_i64() || number.is_u64() {
        return None;
    }
    let float = number.as_f64()?;
    if float.fract() != 0.0 || !float.is_finite() {
        return None;
    }
    if (-I64_END..I64_END).contains(&float) {
        // Integral and within range, so the conversion is exact.
        #[allow(clippy::cast_possible_truncation)]
        return Some(serde_json::Number::from(float as i64));
    }
    if (0.0..U64_END).contains(&float) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        return Some(serde_json::Number::from(float as u64));
    }
    None
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
                    return Err(format!("{spelling:?} is not a {key} map key"));
                }
                check(ir, of, entry, depth + 1).map_err(|error| format!("{spelling:?}: {error}"))
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
                    return Err(format!("{name} declares no member {member:?}"));
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
