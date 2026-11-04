//! Answering a view for one identity from the store of the component that owns its source.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedEntity, ResolvedView};
use ess_domain::types::Presence;
use serde_json::{Map, Value};
use spindle_kernel::Row;

/// Why a view cannot be answered here: the constructs a one-row read by identity does not cover.
pub(crate) fn unsupported(view: &ResolvedView) -> Option<&'static str> {
    if view.filter.is_some() {
        Some("a filter")
    } else if !view.params.is_empty() {
        Some("parameters")
    } else if !view.computed.is_empty() {
        Some("computed fields")
    } else if view.aggregation.is_some() {
        Some("an aggregation")
    } else if !view.references.is_empty() {
        Some("fields read through a reference")
    } else if view.absent.is_some() {
        Some("a default row for an absent identity")
    } else if view.paging.is_some() {
        Some("paging")
    } else {
        None
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
