//! Answering a view from the store of the component that owns its source: one row by identity,
//! or every row the view holds, kept by its filter and ranked by its `order_by`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedEntity, ResolvedTypeRef, ResolvedView};
use ess_conformance::input::Completeness;
use ess_domain::types::{MAX_TYPE_DEPTH, Presence, Primitive};
use ess_domain::view::{Direction, ViewSpec};
use ess_primitives::facts::FactSource;
use ess_primitives::node::Node;
use ess_primitives::predicate::{CompareOp, Operand, Predicate, Truth};
use ess_primitives::{FactPath, FactStore, FactValue, Rfc3339Instant};
use serde_json::{Map, Value};
use spindle_kernel::Row;

/// The one fact path a filter read here may name: the lifecycle state.
const STATE: &str = "state";

/// Why a view cannot be answered here: the constructs a read of the source's own rows does not
/// cover, named.
///
/// A filter is read only where each leaf, as the compiler parsed it, is one of: `state == <State>`,
/// `state != <State>`, `state: {in: [..]}` or `{none_of: [..]}`; a parameter compared with `==`
/// or `!=` to the state, the identity or a field of the source (`book_id == param.id`); or
/// `defined(param.<name>)`; leaves combined by `any`, `all` and `not`. Any other filter is named
/// with its text and refused whole, since evaluating part of it would answer a subset of its rows.
pub(crate) fn unsupported(ir: &EssIr, view: &ResolvedView) -> Option<String> {
    if !view.computed.is_empty() {
        Some("computed fields".to_owned())
    } else if view.aggregation.is_some() {
        Some("an aggregation".to_owned())
    } else if !view.references.is_empty() {
        Some("fields read through a reference".to_owned())
    } else if view.absent.is_some() {
        Some("a default row for an absent identity".to_owned())
    } else if view.paging.is_some() {
        Some("paging".to_owned())
    } else {
        match &view.filter {
            Some(filter) if !reads(ir.entity(&view.source), view, filter) => Some(format!(
                "the filter `{filter}`; only comparisons of `state` with a state, and of a \
                 parameter with the state, the identity or a field by `==` or `!=`, \
                 combined by `any`, `all` and `not`, are read"
            )),
            _ => None,
        }
    }
}

/// Whether every leaf of `filter` is one [`unsupported`] admits: the state compared for equality
/// or inequality with a text literal, or listed among states it is or is not in; a declared
/// parameter compared by `==` or `!=` with the state, the identity or a field of `entity`; or a
/// parameter's `defined`. Every inner node is `any`, `all` or `not`. The parser bounds the nesting
/// at `MAX_PREDICATE_DEPTH`.
fn reads(entity: &ResolvedEntity, view: &ResolvedView, filter: &Predicate) -> bool {
    let is_state = |path: &FactPath| path.segments() == [STATE];
    let all_text = |values: &[FactValue]| {
        values
            .iter()
            .all(|value| matches!(value, FactValue::Text(_)))
    };
    let param = |path: &FactPath| match path.segments() {
        [namespace, name] => {
            namespace == ViewSpec::PARAM && view.params.iter().any(|param| &param.name == name)
        }
        _ => false,
    };
    let row = |path: &FactPath| match path.segments() {
        [name] => entity.observable_field(name).is_some(),
        _ => false,
    };
    match filter {
        Predicate::Compare {
            left: Operand::Fact(left),
            op: CompareOp::Eq | CompareOp::Ne,
            right: Operand::Fact(right),
            ..
        } => (param(left) && row(right)) || (row(left) && param(right)),
        Predicate::Defined(path) => param(path),
        Predicate::Compare {
            left: Operand::Fact(path),
            op: CompareOp::Eq | CompareOp::Ne,
            right: Operand::Literal(FactValue::Text(_)),
            ..
        } => is_state(path),
        Predicate::AnyOf { path, values } | Predicate::NoneOf { path, values } => {
            is_state(path) && all_text(values)
        }
        Predicate::Any(children) | Predicate::All(children) => {
            children.iter().all(|child| reads(entity, view, child))
        }
        Predicate::Not(child) => reads(entity, view, child),
        _ => false,
    }
}

/// The values `given` holds for `view`'s parameters, by declared name, as the facts its filter
/// reads under `param.`. Each value given is checked as ESS's interpreter checks a view parameter
/// (`interpret/views.rs:45` `query`, `input::validate_typed_value`) by [`spindle_kernel::admits`],
/// the check every command input goes through: its type, and a newtype's prefix, alphabet and
/// invariants. It is then bound by ESS ([`ess_conformance::input::bind`]).
///
/// # Errors
/// Naming the parameter: one `view` does not declare, a required one `given` leaves out (or holds
/// as `null`), or one whose value is not of its declared type or breaks its newtype.
pub(crate) fn parameters(
    ir: &EssIr,
    view: &ResolvedView,
    given: &Map<String, Value>,
) -> Result<FactStore, String> {
    if let Some(name) = given
        .keys()
        .find(|name| !view.params.iter().any(|param| &param.name == *name))
    {
        return Err(format!("`{name}` is not a parameter of {}", view.name));
    }
    let mut values = BTreeMap::new();
    for param in &view.params {
        match given.get(&param.name).filter(|value| !value.is_null()) {
            Some(value) => {
                spindle_kernel::admits(ir, &param.type_ref, value)
                    .map_err(|error| format!("`{}`: {error}", param.name))?;
                let node = serde_json::from_value::<Node>(value.clone())
                    .map_err(|error| format!("`{}`: {error}", param.name))?;
                values.insert(param.name.clone(), node);
            }
            None if param.type_ref.is_optional() => {}
            None => return Err(format!("the parameter `{}` is required", param.name)),
        }
    }
    ess_conformance::input::bind(ir, &view.params, &values, Completeness::Partial)
        .map_err(|errors| format!("the parameter {errors}"))
}

/// What a filter reads: the row's facts at their own paths, the parameters under `param.`.
struct Read<'a> {
    row: &'a FactStore,
    parameters: &'a FactStore,
}

impl Read<'_> {
    fn source<'p>(&self, path: &'p FactPath) -> (&FactStore, std::borrow::Cow<'p, FactPath>) {
        match path.segments() {
            [namespace, rest @ ..] if namespace == ViewSpec::PARAM && !rest.is_empty() => (
                self.parameters,
                std::borrow::Cow::Owned(FactPath::from_segments(rest)),
            ),
            _ => (self.row, std::borrow::Cow::Borrowed(path)),
        }
    }
}

impl FactSource for Read<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        let (source, path) = self.source(path);
        source.fact(&path)
    }

    fn present(&self, path: &FactPath) -> bool {
        let (source, path) = self.source(path);
        source.present(&path)
    }
}

/// Whether the view holds `row`, the instance `identity` names: always without a filter; with
/// one, where ESS evaluates the filter to true over the row and `parameters`
/// ([`parameters`]). Only a filter [`unsupported`] admits is read.
///
/// A view without parameters reads the state alone, the only fact such a filter reads; one with
/// them binds the state, the identity and every stored field, as ESS binds a source row
/// ([`ess_conformance::input::bind`] over the entity's observable fields).
///
/// A filter unknown for the row is refused, as ESS's interpreter refuses the read
/// (`ess-conformance` `src/interpret/views.rs:214` at ESS 0.87.0, `select`,
/// `Truth::Unknown => return Err(unsupported())`): one comparing with `==` or `!=` against an
/// `Optional` parameter the read left out is unknown, not false. The interpreter hides such a row
/// only where the omitted parameter is read by a string operator (`absent_text_operands`,
/// `views.rs:210`), and no filter [`unsupported`] admits has one.
///
/// # Errors
/// When the row is not a value of the source's declared fields, naming what it holds, or when the
/// filter is unknown for it.
pub(crate) fn holds(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
    parameters: &FactStore,
) -> Result<bool, String> {
    let Some(filter) = &view.filter else {
        return Ok(true);
    };
    let facts = if view.params.is_empty() {
        let mut facts = FactStore::new();
        if let Ok(path) = FactPath::new(STATE) {
            facts.set(path, FactValue::text(row.state.clone()));
        }
        facts
    } else {
        let entity = ir.entity(&view.source);
        let mut held = row
            .fields
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        held.insert(entity.identity.name.clone(), identity.clone());
        held.insert(STATE.to_owned(), Value::String(row.state.clone()));
        let held: BTreeMap<String, Node> = serde_json::from_value(Value::Object(held))
            .map_err(|error| format!("a row of {}: {error}", entity.name))?;
        ess_conformance::input::bind(
            ir,
            &entity.observable_fields(),
            &held,
            Completeness::Partial,
        )
        .map_err(|errors| format!("a row of {}: {errors}", entity.name))?
    };
    let read = Read {
        row: &facts,
        parameters,
    };
    match filter.evaluate(&read) {
        Truth::True => Ok(true),
        Truth::False => Ok(false),
        Truth::Unknown => Err(format!(
            "the filter `{filter}` is unknown for a row, so the model does not determine the rows \
             it holds"
        )),
    }
}

/// The view's row for the instance `identity` names, held as `row`.
///
/// A field typed as the source's lifecycle state answers the state the row holds; the field named
/// as the source's identity answers `identity`; every other field answers the stored field of that
/// name. Where the row holds nothing for it, the field follows the presence the compiler carried
/// onto it from its source field: `null` under `null_when_absent`, left out otherwise.
pub(crate) fn project(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
) -> Map<String, Value> {
    let entity: &ResolvedEntity = ir.entity(&view.source);
    let mut answer = Map::new();
    for field in &view.fields {
        let value = if field.type_ref.required().declared() == Some(&entity.state_type) {
            Some(Value::String(row.state.clone()))
        } else if field.name == entity.identity.name {
            Some(identity.clone())
        } else {
            row.fields
                .get(&field.name)
                .filter(|value| !value.is_null())
                .cloned()
        };
        match value {
            Some(value) => {
                answer.insert(field.name.clone(), value);
            }
            None if field.naming.presence == Some(Presence::NullWhenAbsent) => {
                answer.insert(field.name.clone(), Value::Null);
            }
            None => {}
        }
    }
    answer
}

/// Sorts `rows`, each an identity and the view's row for it, into the order the view answers:
/// by each `order_by` key in turn, descending where the key says so, then by identity ascending,
/// so rows tied on every key, and every row of a view without `order_by`, come in one order on
/// every store.
///
/// A key ranks by its field's declared type, through `Optional` and newtypes: a `Timestamp` by the
/// instant it names, not its spelling, and one that names no instant below every one that does;
/// a number by its exact value; anything else by its JSON value,
/// text by its UTF-8 bytes. A field a row does not carry ranks below every value.
pub(crate) fn order(ir: &EssIr, view: &ResolvedView, rows: &mut [(Value, Map<String, Value>)]) {
    let entity = ir.entity(&view.source);
    let identity = primitive(ir, &entity.identity.type_ref);
    let keys = view
        .order_by
        .iter()
        .map(|ranking| {
            let kind = view
                .fields
                .iter()
                .find(|field| field.name == ranking.field)
                .and_then(|field| primitive(ir, &field.type_ref));
            (ranking.field.as_str(), ranking.direction, kind)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|(left_id, left), (right_id, right)| {
        for (field, direction, kind) in &keys {
            let ordering = rank(*kind, left.get(*field), right.get(*field));
            let ordering = match direction {
                Direction::Ascending => ordering,
                Direction::Descending => ordering.reverse(),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        rank(identity, Some(left_id), Some(right_id))
    });
}

/// The primitive `type_ref` is underneath `Optional` and every newtype; `None` for an enum, a
/// struct, a list or anything else that is not a primitive.
fn primitive(ir: &EssIr, type_ref: &ResolvedTypeRef) -> Option<Primitive> {
    let mut current = type_ref.required();
    for _ in 0..=MAX_TYPE_DEPTH {
        match current {
            ResolvedTypeRef::Primitive { name } => return Some(*name),
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype { of, .. } => current = of.required(),
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}

/// How two values of one key rank; an absent value, or `null`, below every value.
fn rank(kind: Option<Primitive>, left: Option<&Value>, right: Option<&Value>) -> Ordering {
    let left = left.filter(|value| !value.is_null());
    let right = right.filter(|value| !value.is_null());
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => value(kind, left, right),
    }
}

/// How two present values of one key rank. Every rule is a total order, as a sort needs: one
/// rule that is not lets a third row reorder two others.
///
/// - `Timestamp`: a value that parses as an RFC 3339 instant ranks by that instant; one that does
///   not (ESS admits any text as a `Timestamp`) ranks below every one that does, and two such
///   values rank by their JSON value as below. Two spellings of one instant tie.
/// - Numbers rank by their exact value, integer against float included, never through a lossy
///   conversion.
/// - Text ranks by its UTF-8 bytes, booleans `false` before `true`, and any other pair by the
///   bytes of its JSON rendering.
fn value(kind: Option<Primitive>, left: &Value, right: &Value) -> Ordering {
    if kind == Some(Primitive::Timestamp) {
        let instant = |value: &Value| value.as_str().and_then(Rfc3339Instant::parse_rfc3339);
        match (instant(left), instant(right)) {
            (Some(left), Some(right)) => return left.cmp(&right),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => {}
        }
    }
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => number(left, right),
        (Value::String(left), Value::String(right)) => left.as_bytes().cmp(right.as_bytes()),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (left, right) => left.to_string().cmp(&right.to_string()),
    }
}

/// Two JSON numbers by exact value. Integers compare as `i128`, which holds every `i64` and
/// `u64`; an integer against a float compares through the float only where the rounded integer
/// differs from it, since rounding keeps order; where they are equal the float is integral and
/// compares exactly as an integer.
fn number(left: &serde_json::Number, right: &serde_json::Number) -> Ordering {
    let integer = |number: &serde_json::Number| {
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
    };
    let against = |integer: i128, float: Option<f64>| {
        float.map_or(Ordering::Equal, |float| {
            match (integer as f64).partial_cmp(&float) {
                Some(Ordering::Equal) => integer.cmp(&(float as i128)),
                ordering => ordering.unwrap_or(Ordering::Equal),
            }
        })
    };
    match (integer(left), integer(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(left), None) => against(left, right.as_f64()),
        (None, Some(right)) => against(right, left.as_f64()).reverse(),
        (None, None) => left
            .as_f64()
            .zip(right.as_f64())
            .and_then(|(left, right)| left.partial_cmp(&right))
            .unwrap_or(Ordering::Equal),
    }
}
