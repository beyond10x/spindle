//! Answering a view from the store of the component that owns its source: one row by identity,
//! or every row the view holds, kept by its filter and ranked by its `order_by`.

use std::cmp::Ordering;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedEntity, ResolvedTypeRef, ResolvedView};
use ess_domain::types::{MAX_TYPE_DEPTH, Presence, Primitive};
use ess_domain::view::Direction;
use ess_primitives::predicate::{CompareOp, Operand, Predicate, Truth};
use ess_primitives::{FactPath, FactStore, FactValue, Rfc3339Instant};
use serde_json::{Map, Value};
use spindle_kernel::Row;

/// The one fact path a filter read here may name: the lifecycle state.
const STATE: &str = "state";

/// Why a view cannot be answered here: the constructs a read of the source's own rows does not
/// cover, named.
///
/// A filter is read only where it reads the lifecycle state and nothing else, as the compiler
/// parsed it: `state == <State>`, `state != <State>`, `state: {in: [..]}` or `{none_of: [..]}`,
/// combined by `any`, `all` and `not`. Any other filter is named with its text and refused whole,
/// since evaluating it over the state alone would answer a subset of its rows.
pub(crate) fn unsupported(view: &ResolvedView) -> Option<String> {
    if !view.params.is_empty() {
        Some("parameters".to_owned())
    } else if !view.computed.is_empty() {
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
            Some(filter) if !reads_only_state(filter) => Some(format!(
                "the filter `{filter}`; only comparisons of `state` with a state, \
                 combined by `any`, `all` and `not`, are read"
            )),
            _ => None,
        }
    }
}

/// Whether `filter` reads the state fact and nothing else: each leaf compares it for equality or
/// inequality with a text literal, or lists the states it is or is not among, and every inner
/// node is `any`, `all` or `not`. The parser bounds the nesting at `MAX_PREDICATE_DEPTH`.
fn reads_only_state(filter: &Predicate) -> bool {
    let is_state = |path: &FactPath| path.segments() == [STATE];
    let all_text = |values: &[FactValue]| {
        values
            .iter()
            .all(|value| matches!(value, FactValue::Text(_)))
    };
    match filter {
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
            children.iter().all(reads_only_state)
        }
        Predicate::Not(child) => reads_only_state(child),
        _ => false,
    }
}

/// Whether the view holds `row`: always without a filter; with one, where ESS evaluates the
/// filter to true over the state the row holds. Only a filter [`unsupported`] admits is read, and
/// that filter reads nothing but the state.
pub(crate) fn holds(view: &ResolvedView, row: &Row) -> bool {
    let Some(filter) = &view.filter else {
        return true;
    };
    let mut facts = FactStore::new();
    if let Ok(path) = FactPath::new(STATE) {
        facts.set(path, FactValue::text(row.state.clone()));
    }
    filter.evaluate(&facts) == Truth::True
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
