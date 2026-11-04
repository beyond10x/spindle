//! What the kernel runs, checked before a command runs.
//!
//! The kernel runs the conditions, effects and value sources Spindle's own specification and the
//! `rally` example use. Every other construct is refused by name, as
//! [`ExecuteError::Unsupported`], before anything is read or written: a command that declares one
//! is not run at all, because selecting among its outcomes without understanding one of them would
//! be a guess.

use ess_compiler::ir::{
    ResolvedCommand, ResolvedCondition, ResolvedEffect, ResolvedInstance, ResolvedOutcome,
    ResolvedPayloadField, ResolvedPayloadValue,
};

use crate::ExecuteError;

/// Refuses `command` when any outcome declares a construct the kernel does not run.
pub(crate) fn check(command: &ResolvedCommand) -> Result<(), ExecuteError> {
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
        for field in outcome
            .sets
            .iter()
            .chain(outcome.payload.iter().flat_map(|payload| &payload.fields))
            .chain(&outcome.error_payload)
        {
            if let Some(what) = value(field) {
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

/// The value source, named, when the kernel does not read it.
fn value(field: &ResolvedPayloadField) -> Option<String> {
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
        | ResolvedPayloadValue::EmptyList
        | ResolvedPayloadValue::Cleared => None,
        ResolvedPayloadValue::Struct { fields } => fields.iter().find_map(value),
        other => Some(format!("the value source {}", other.describe())),
    }
}
