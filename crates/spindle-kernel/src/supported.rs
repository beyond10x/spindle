//! What the kernel runs, checked before a command runs.
//!
//! The kernel runs the conditions, effects and value sources Spindle's own specification and the
//! `rally` example use. Every other construct is refused by name, as
//! [`ExecuteError::Unsupported`], before anything is read or written: a command that declares one
//! is not run at all, because selecting among its outcomes without understanding one of them would
//! be a guess.

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedCommand, ResolvedCondition, ResolvedEffect, ResolvedInstance, ResolvedOutcome,
    ResolvedPayloadField, ResolvedPayloadValue,
};

use crate::ExecuteError;

/// Refuses `command` when any outcome declares a construct the kernel does not run.
pub(crate) fn check(ir: &EssIr, command: &ResolvedCommand) -> Result<(), ExecuteError> {
    for outcome in &command.outcomes {
        let refuse = |what: &str| {
            Err(ExecuteError::Unsupported {
                construct: format!("{what}, in `{}/{}`", command.name, outcome.name.as_str()),
            })
        };
        if let Some(what) = condition(&outcome.condition) {
            return refuse(what);
        }
        if let Some(what) = shape(outcome) {
            return refuse(what);
        }
        let minted = crate::mint::slot(ir, outcome);
        if let Some((entity, field)) = minted {
            let Some(shape) = crate::mint::identity(ir, &entity.identity.type_ref) else {
                return refuse(&format!(
                    "a creation whose identity `{}.{}` of type `{}` neither the input nor the \
                     outcome determines; the runtime mints a `Uuid` identity or a newtype of one \
                     without invariants, or a `String` newtype whose prefix and alphabet admit a \
                     UUID's text and whose invariants bound only a length that text meets",
                    entity.name, field.name, entity.identity.type_ref
                ));
            };
            if let Some(bound) = crate::mint::identity_bound(entity, &shape) {
                return refuse(&format!(
                    "a creation whose identity `{}.{}` of type `{}` the runtime mints, but the \
                     entity's invariant `{}` holds for no minted identity",
                    entity.name, field.name, entity.identity.type_ref, bound.statement
                ));
            }
        }
        // `{generated: true}` in the top-level field `instance:` names, on the event it names, is
        // the identity mint; anywhere else it is a generated value of its own.
        let creating = minted.and(
            match outcome.subject.as_ref().map(|subject| &subject.instance) {
                Some(ResolvedInstance::Observed { event, .. }) => Some(event),
                _ => None,
            },
        );
        let identity_fields = outcome
            .payload
            .iter()
            .filter(|payload| creating.is_some_and(|event| *event == payload.event))
            .flat_map(|payload| &payload.fields);
        let other_fields = outcome
            .payload
            .iter()
            .filter(|payload| creating.is_none_or(|event| *event != payload.event))
            .flat_map(|payload| &payload.fields);
        for field in identity_fields {
            if matches!(field.value, ResolvedPayloadValue::Generated { .. })
                && minted.is_some_and(|(_, identity)| identity.name == field.target)
                && field.conversion.is_none()
            {
                continue;
            }
            if let Some(what) = value(ir, field) {
                return refuse(&what);
            }
        }
        for field in outcome
            .sets
            .iter()
            .chain(other_fields)
            .chain(&outcome.error_payload)
        {
            if let Some(what) = value(ir, field) {
                return refuse(&what);
            }
        }
    }
    Ok(())
}

/// The condition, named, when the kernel does not run it.
fn condition(condition: &ResolvedCondition) -> Option<&'static str> {
    match condition {
        ResolvedCondition::Otherwise
        | ResolvedCondition::External { .. }
        | ResolvedCondition::ExternalWhen { .. }
        | ResolvedCondition::ExternalSubject { .. }
        | ResolvedCondition::When { .. }
        | ResolvedCondition::WrongState
        | ResolvedCondition::UnknownInstance
        | ResolvedCondition::SubjectPredicate { .. }
        | ResolvedCondition::SubjectField { .. }
        | ResolvedCondition::SubjectState { .. } => None,
        ResolvedCondition::Related { .. } => Some("`when_related:` over one row"),
        ResolvedCondition::RelatedSet { .. } => Some("`when_related:` over a row set"),
        ResolvedCondition::StateChange { .. } => Some("`when_state_changes:`"),
        ResolvedCondition::InputAbsent => Some("`input_absent:`"),
        ResolvedCondition::ExistingInstance => Some("`existing_instance:`"),
    }
}

/// The outcome's shape, named, when the kernel does not run it.
fn shape(outcome: &ResolvedOutcome) -> Option<&'static str> {
    if outcome.replays.is_some() || outcome.retains_result {
        return Some("a replayed or retained result");
    }
    if outcome.returns || !outcome.one_time_response.is_empty() {
        return Some("a command response");
    }
    if outcome.compensates {
        return Some("a refusal that changes its row (`compensates: true`)");
    }
    if outcome.before_row {
        return Some("an external refusal before the row (`before_row: true`)");
    }
    if outcome.instances.is_some() {
        return Some("a set outcome (`instances:`)");
    }
    if !outcome.affects.is_empty() {
        return Some("secondary effects (`affects:`)");
    }
    if outcome.decided_by_caller {
        return Some("a branch the caller decides");
    }
    match &outcome.subject {
        None if !outcome.sets.is_empty() => Some("`sets:` without a subject"),
        None => None,
        Some(subject) => match (&subject.effect, &subject.instance) {
            (ResolvedEffect::Creates, ResolvedInstance::Observed { .. })
            | (
                ResolvedEffect::Moves { .. } | ResolvedEffect::Updates,
                ResolvedInstance::Supplied { .. },
            ) => None,
            (ResolvedEffect::Preserves, ResolvedInstance::Supplied { .. })
                if outcome.sets.is_empty() =>
            {
                None
            }
            (ResolvedEffect::Preserves, _) => Some("`preserves:` beside `sets:`"),
            (ResolvedEffect::Deletes, _) => Some("`deletes:`"),
            (ResolvedEffect::Creates, ResolvedInstance::Supplied { .. }) => {
                Some("`creates:` with a supplied identity")
            }
            (_, ResolvedInstance::Observed { .. }) => {
                Some("an observed identity on an existing row")
            }
        },
    }
}

/// The value source, named, when the kernel does not read it. A generated value is read where
/// its type is one the runtime mints ([`crate::mint::minted`]), and the refusal of one that is
/// not names the field and its type.
fn value(ir: &EssIr, field: &ResolvedPayloadField) -> Option<String> {
    let mintable = |present: bool| {
        (!crate::mint::absent(&field.target_type, present)
            && crate::mint::minted(ir, &field.target_type, present).is_none())
        .then(|| crate::mint::unmintable(&field.target, &field.target_type))
    };
    if let Some(conversion) = &field.conversion {
        return Some(format!(
            "the conversion `{conversion}` into `{}`",
            field.target
        ));
    }
    match &field.value {
        ResolvedPayloadValue::InputField { .. }
        | ResolvedPayloadValue::Literal { .. }
        | ResolvedPayloadValue::SubjectField { .. }
        | ResolvedPayloadValue::SubjectState { .. }
        | ResolvedPayloadValue::Increment { .. }
        | ResolvedPayloadValue::Append { .. }
        | ResolvedPayloadValue::EmptyList
        | ResolvedPayloadValue::PayloadRead { .. }
        | ResolvedPayloadValue::Cleared => None,
        ResolvedPayloadValue::Generated { present } => mintable(*present),
        ResolvedPayloadValue::InputOrGenerated {
            otherwise: None, ..
        } => mintable(false),
        ResolvedPayloadValue::Struct { fields } => {
            fields.iter().find_map(|member| value(ir, member))
        }
        other => Some(format!("the value source {}", other.describe())),
    }
}
