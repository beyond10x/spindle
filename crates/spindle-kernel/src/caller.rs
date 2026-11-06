//! The caller a command is sent as (ess/16): which actor it is, what its credential carries, and
//! where a command reads it.
//!
//! A caller is bound as ESS's interpreter binds one (`interpret::execute::caller::Caller::bind`):
//! against the attributes of the actor it is sent as, by ESS's own
//! [`bind`](ess_conformance::input::bind) with [`Completeness::Total`], so an attribute that actor
//! does not declare, a value its type does not admit and a required attribute left out are each
//! refused before the command reads anything. Attributes sent as no actor are refused; an empty
//! caller sent as no actor is no caller at all.
//!
//! [`reads`] names every attribute a command reads, wherever a read can sit: a guard of any
//! condition and any value source of `sets:`, a payload or an error's payload.

use std::collections::{BTreeMap, BTreeSet};

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedCommand, ResolvedCondition, ResolvedField, ResolvedPayloadField, ResolvedPayloadValue,
    ResolvedRelatedTest,
};
use ess_conformance::input::{Completeness, bind as bind_values};
use ess_primitives::node::Node;
use ess_primitives::predicate::Predicate;
use serde_json::{Map, Value};

use crate::ExecuteError;
use crate::facts::CALLER;

/// Binds `caller`, sent as `actor`, and answers the attributes that actor declares: what a guard's
/// `caller.` operand is typed by. A command sent as no caller binds nothing.
///
/// # Errors
/// [`ExecuteError::InvalidInput`] for attributes sent as no declared actor, and for a caller that
/// is not one of its actor's: an undeclared attribute, a value its type does not admit, or a
/// required attribute left out.
pub(crate) fn bind<'ir>(
    ir: &'ir EssIr,
    actor: Option<&str>,
    caller: Option<&Map<String, Value>>,
) -> Result<&'ir [ResolvedField], ExecuteError> {
    let declared = actor.and_then(|actor| {
        ir.actors()
            .iter()
            .find_map(|(name, declared)| (name.to_string() == actor).then_some(declared))
    });
    let Some(caller) = caller else {
        return Ok(declared.map_or(&[][..], |actor| actor.attributes.as_slice()));
    };
    let Some(declared) = declared else {
        return if caller.is_empty() {
            Ok(&[])
        } else {
            Err(ExecuteError::InvalidInput(
                "caller attributes require a declared actor".to_owned(),
            ))
        };
    };
    let values = caller
        .iter()
        .map(|(name, value)| {
            serde_json::from_value::<Node>(value.clone())
                .map(|node| (name.clone(), node))
                .map_err(|error| {
                    ExecuteError::InvalidInput(format!("the caller's `{name}`: {error}"))
                })
        })
        .collect::<Result<BTreeMap<String, Node>, _>>()?;
    bind_values(ir, &declared.attributes, &values, Completeness::Total).map_err(|error| {
        ExecuteError::InvalidInput(format!(
            "the caller is not a caller of `{}`: {error}",
            declared.name
        ))
    })?;
    Ok(&declared.attributes)
}

/// Refuses `caller` sent as `actor` exactly as [`execute_as`](crate::execute_as) refuses it before
/// a command reads anything, without running one: so a caller fixed once for many commands can be
/// refused before the first.
///
/// # Errors
/// As the caller binding of [`execute_as`](crate::execute_as): [`ExecuteError::InvalidInput`]
/// for attributes sent as no declared actor, an undeclared attribute, a value its type does not
/// admit, or a required attribute left out.
pub fn binds(
    ir: &EssIr,
    actor: Option<&str>,
    caller: Option<&Map<String, Value>>,
) -> Result<(), ExecuteError> {
    bind(ir, actor, caller).map(|_| ())
}

/// Every caller attribute `command` reads, each once, in name order: a `caller.<attribute>`
/// operand of any condition's guard whose fields declare no `caller` of their own, and every
/// `{caller: <attribute>}` of `sets:`, a payload or an error's payload, struct members included.
pub fn reads(ir: &EssIr, command: &ResolvedCommand) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for outcome in &command.outcomes {
        let subject = command
            .selection_subject(outcome)
            .map(|subject| ir.entity(&subject.entity).fields.as_slice())
            .unwrap_or_default();
        let mut guards: Vec<(&Predicate, &[ResolvedField])> = Vec::new();
        match &outcome.condition {
            ResolvedCondition::When { predicate }
            | ResolvedCondition::ExternalWhen { predicate, .. } => {
                guards.push((predicate, &command.input));
            }
            ResolvedCondition::SubjectPredicate { predicate, input }
            | ResolvedCondition::ExternalSubject {
                predicate, input, ..
            } => {
                guards.push((predicate, subject));
                guards.extend(input.iter().map(|input| (input, command.input.as_slice())));
            }
            ResolvedCondition::SubjectField { predicate, .. }
            | ResolvedCondition::SubjectState { predicate, .. }
            | ResolvedCondition::StateChange { predicate, .. } => {
                guards.extend(
                    predicate
                        .iter()
                        .map(|input| (input, command.input.as_slice())),
                );
            }
            ResolvedCondition::Related {
                entity,
                test,
                input,
                also,
                ..
            } => {
                if let ResolvedRelatedTest::Holds { predicate } = test {
                    guards.push((predicate, &ir.entity(entity).fields));
                }
                for row in also {
                    guards.push((&row.predicate, &ir.entity(&row.entity).fields));
                }
                guards.extend(input.iter().map(|input| (input, command.input.as_slice())));
            }
            ResolvedCondition::RelatedSet { input, .. } => {
                guards.extend(input.iter().map(|input| (input, command.input.as_slice())));
            }
            ResolvedCondition::Otherwise
            | ResolvedCondition::External { .. }
            | ResolvedCondition::WrongState
            | ResolvedCondition::UnknownInstance
            | ResolvedCondition::InputAbsent
            | ResolvedCondition::ExistingInstance => {}
        }
        for (predicate, roots) in guards {
            if roots.iter().any(|field| field.name == CALLER) {
                continue;
            }
            for path in predicate.fact_paths() {
                if let [namespace, attribute] = path.segments()
                    && namespace.as_str() == CALLER
                {
                    found.insert(attribute.as_str().to_owned());
                }
            }
        }
        for field in outcome
            .sets
            .iter()
            .chain(outcome.payload.iter().flat_map(|payload| &payload.fields))
            .chain(&outcome.error_payload)
        {
            values(field, &mut found);
        }
    }
    found
}

fn values(field: &ResolvedPayloadField, found: &mut BTreeSet<String>) {
    match &field.value {
        ResolvedPayloadValue::CallerAttribute { attribute, .. } => {
            found.insert(attribute.clone());
        }
        ResolvedPayloadValue::Struct { fields } => {
            for member in fields {
                values(member, found);
            }
        }
        _ => {}
    }
}
