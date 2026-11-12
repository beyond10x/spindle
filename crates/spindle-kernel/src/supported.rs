//! What the kernel runs, checked before a command runs.
//!
//! The kernel runs the conditions, effects and value sources Spindle's own specification and the
//! `rally` example use, a value read from the row an input field names, a value an expression
//! computes outside an error, a guard over a row set, a set outcome (`instances:`) with the rows
//! it changed, secondary effects (`affects:`) selecting their rows by a filter, and an event
//! payload value read from the response the outcome answers (`{response: …}`). Every other
//! construct is refused by name, as [`ExecuteError::Unsupported`], before anything is written;
//! what it refuses depends on what reads the construct.
//!
//! A construct selection reads refuses the whole command before anything is read ([`selects`]):
//! a condition, a `when_related:` read two ways, or the shape of an outcome (the rows it acts on,
//! when its cause is asked, a retained result). Selecting among the outcomes without
//! understanding one of them would be a guess.
//!
//! A construct only the taken outcome reads refuses that outcome alone ([`takes`]): an effect
//! beyond its subject (a compensating refusal, an `affects:` entry over an input list, `sets:`
//! without a subject), a value it
//! writes, publishes or answers, the identity it mints, or an error field it fills. Selection
//! never reads one, so a request that selects another outcome of the command runs, and only a
//! request that selects this one is refused, once it is selected and before anything is minted
//! or written.

use std::collections::BTreeSet;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EntityHandle, EventHandle, ResolvedCommand, ResolvedCondition, ResolvedEffect,
    ResolvedInstance, ResolvedOutcome, ResolvedPayloadField, ResolvedPayloadValue,
    ResolvedRelatedVia, ResolvedTypeRef,
};

use crate::ExecuteError;

/// Refuses `command` when it or any of its outcomes declares a construct the kernel does not run,
/// outcome by outcome in declaration order: what [`selects`] refuses, and what [`takes`] refuses
/// for any outcome. A host asks it to know whether every request of the command runs.
///
/// # Errors
/// [`ExecuteError::Unsupported`], naming the construct and the outcome that declares it.
pub fn check(ir: &EssIr, command: &ResolvedCommand) -> Result<(), ExecuteError> {
    mixed(command)?;
    for outcome in &command.outcomes {
        selecting(command, outcome)?;
        takes(ir, command, outcome)?;
    }
    Ok(())
}

/// Refuses `command` when it declares a construct selection reads and the kernel does not run:
/// the check [`execute`](crate::execute) makes before it reads anything, and the one a host asks
/// before it serves the command at all.
///
/// # Errors
/// [`ExecuteError::Unsupported`], naming the construct and the outcome that declares it.
pub fn selects(command: &ResolvedCommand) -> Result<(), ExecuteError> {
    mixed(command)?;
    command
        .outcomes
        .iter()
        .try_for_each(|outcome| selecting(command, outcome))
}

/// Refuses `outcome` of `command` when it declares a construct only the taken outcome reads and
/// the kernel does not run: the check [`execute`](crate::execute) makes once a request selects
/// the outcome, before it mints or writes anything, and the one a host asks to name each outcome
/// it does not run while it serves the command's others.
///
/// # Errors
/// [`ExecuteError::Unsupported`], naming the construct and the outcome.
pub fn takes(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
) -> Result<(), ExecuteError> {
    let refuse = |what: &str| Err(unsupported(command, outcome, what));
    let set = outcome.instances.is_some();
    if let Some(what) = effect(outcome) {
        return refuse(what);
    }
    if let Some(what) = response(ir, command, outcome) {
        return refuse(&what);
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
    // An event payload of an outcome that answers the response reads it (ESS
    // `interpret::execute::take` prepares the response before the creation and before `emit`);
    // `sets:`, `affects:` and an error's fields do not.
    let answers = crate::mint::answers(outcome);
    let reading = Reading {
        set,
        error: false,
        response: answers,
    };
    for field in identity_fields {
        if matches!(field.value, ResolvedPayloadValue::Generated { .. })
            && minted.is_some_and(|(_, identity)| identity.name == field.target)
            && field.conversion.is_none()
        {
            continue;
        }
        if let Some(what) = value(ir, field, reading) {
            return refuse(&what);
        }
    }
    let stored = Reading {
        response: false,
        ..reading
    };
    let fields = outcome
        .sets
        .iter()
        .chain(outcome.affects.iter().flat_map(|affect| &affect.sets))
        .map(|field| (field, stored))
        .chain(other_fields.map(|field| (field, reading)));
    let errors = outcome.error_payload.iter().map(|field| {
        (
            field,
            Reading {
                error: true,
                ..stored
            },
        )
    });
    for (field, reading) in fields.chain(errors) {
        if let Some(what) = value(ir, field, reading) {
            return refuse(&what);
        }
    }
    if let Some(what) = published_response(outcome) {
        return refuse(&what);
    }
    if let Some(what) = unfilled_error_field(ir, command, outcome) {
        return refuse(&what);
    }
    Ok(())
}

/// Refuses a command whose `when_related:` branches read rows two ways ([`mixed_references`]),
/// or whose `existing_instance:` branch stands beside creations of different addresses
/// ([`different_addresses`]).
fn mixed(command: &ResolvedCommand) -> Result<(), ExecuteError> {
    if let Some(outcome) = mixed_references(command) {
        return Err(unsupported(
            command,
            outcome,
            "`when_related:` over rows named both by the input and by a stored field",
        ));
    }
    match different_addresses(command) {
        Some(outcome) => Err(unsupported(
            command,
            outcome,
            "`existing_instance:` beside creations of different addresses",
        )),
        None => Ok(()),
    }
}

/// The entity a creation `outcome` creates and the input field its identity is read from: the
/// source its payload gives the event field `instance:` names, where that is an input field (ESS
/// `interpret::execute::existence::identity_source`). `None` where `outcome` creates nothing.
pub(crate) fn creation_address(outcome: &ResolvedOutcome) -> Option<(&EntityHandle, Option<&str>)> {
    let subject = outcome
        .subject
        .as_ref()
        .filter(|subject| subject.effect == ResolvedEffect::Creates)?;
    let field = match &subject.instance {
        ResolvedInstance::Observed { event, field } => outcome
            .payload
            .iter()
            .filter(|payload| payload.event == *event)
            .flat_map(|payload| &payload.fields)
            .find(|source| source.target == field.name)
            .and_then(|source| match &source.value {
                ResolvedPayloadValue::InputField { field, .. }
                | ResolvedPayloadValue::InputOrGenerated { field, .. } => Some(field.as_str()),
                _ => None,
            }),
        ResolvedInstance::Supplied { .. } => None,
    };
    Some((&subject.entity, field))
}

/// The `existing_instance:` branch of a command whose creations take their identities at
/// different addresses. ESS's interpreter reads which address is taken only by selecting the
/// accepting branch first (`interpret::execute::existence::existing`), and the kernel does not
/// guess at it.
fn different_addresses(command: &ResolvedCommand) -> Option<&ResolvedOutcome> {
    let existing = command
        .outcomes
        .iter()
        .find(|outcome| outcome.condition == ResolvedCondition::ExistingInstance)?;
    let addresses: BTreeSet<_> = command
        .outcomes
        .iter()
        .filter_map(creation_address)
        .collect();
    (addresses.len() > 1).then_some(existing)
}

/// Refuses `outcome` when its condition or its shape is one the kernel does not run: what
/// selection reads of it.
fn selecting(command: &ResolvedCommand, outcome: &ResolvedOutcome) -> Result<(), ExecuteError> {
    match condition(&outcome.condition).or_else(|| shape(outcome)) {
        Some(what) => Err(unsupported(command, outcome, what)),
        None => Ok(()),
    }
}

/// `what`, which the kernel does not run, named where `outcome` of `command` declares it.
fn unsupported(command: &ResolvedCommand, outcome: &ResolvedOutcome, what: &str) -> ExecuteError {
    ExecuteError::Unsupported {
        construct: format!("{what}, in `{}/{}`", command.name, outcome.name.as_str()),
    }
}

/// The error field, named, that `outcome` gives no source, that no row it is answered for holds
/// and whose type the runtime does not mint: a refusal would answer without a field its
/// published schema requires ([`spindle_capability::mint::unsourced`]).
fn unfilled_error_field(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
) -> Option<String> {
    let error = outcome.error.as_ref()?.name().to_string();
    spindle_capability::mint::unsourced(ir, command, outcome)
        .into_iter()
        .find(|(_, fill)| *fill == spindle_capability::mint::Unsourced::Unfilled)
        .map(|(field, _)| crate::mint::unfilled(&error, field))
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
        | ResolvedCondition::ExistingInstance
        | ResolvedCondition::SubjectPredicate { .. }
        | ResolvedCondition::SubjectField { .. }
        | ResolvedCondition::SubjectState { .. }
        // `when_state_changes:` reads as a held-state guard over the states the compiler derived
        // from the branch's own move ([`crate::execute`]); ESS refuses it beside no `moves:`.
        | ResolvedCondition::StateChange { .. } => None,
        ResolvedCondition::Related { also, .. } if !also.is_empty() => {
            Some("`when_related: {all: […]}` over several rows")
        }
        ResolvedCondition::Related { via, .. }
            if matches!(via.type_ref(), ResolvedTypeRef::List { .. }) =>
        {
            Some("`when_related:` over a list of references")
        }
        ResolvedCondition::Related { .. } => None,
        // A row set is read from the store before selection ([`crate::execute`]); a selector or a
        // `forall:` that walks the rows of an entity (`exists: {in: {entity: …}, …}`, ess/27)
        // stays refused.
        ResolvedCondition::RelatedSet {
            selection, test, ..
        } if std::iter::once(&selection.filter)
            .chain(test.predicate())
            .any(|predicate| !ess_domain::command::related_rows::walked(predicate).is_empty()) =>
        {
            Some("a rows quantifier in a `when_related:` row-set selector")
        }
        ResolvedCondition::RelatedSet { .. } => None,
        ResolvedCondition::InputAbsent => Some("`input_absent:`"),
    }
}

/// The first `when_related:` branch over a row the input names, where another names its row by a
/// stored field of the addressed subject. ESS's interpreter reads only the stored row on such a
/// command (`interpret::execute::related::stored`), so the kernel does not guess at the other.
fn mixed_references(command: &ResolvedCommand) -> Option<&ResolvedOutcome> {
    let via = |outcome: &&ResolvedOutcome| match &outcome.condition {
        ResolvedCondition::Related { via, .. } => {
            Some(matches!(via, ResolvedRelatedVia::Subject { .. }))
        }
        _ => None,
    };
    let stored = command
        .outcomes
        .iter()
        .any(|outcome| via(&outcome) == Some(true));
    command
        .outcomes
        .iter()
        .find(|outcome| stored && via(outcome) == Some(false))
}

/// The effect beyond the outcome's subject, named, when the kernel does not run it: the change a
/// refusal makes to its row, a secondary effect ([`affects`]), or fields it sets with no subject
/// to set them on and no set of rows (`instances:`). Neither the kernel's selection nor ESS's
/// precedence plan reads one (`compensates`, `affects:` and `sets:` are read only when the
/// outcome is taken), so it refuses the outcome alone.
fn effect(outcome: &ResolvedOutcome) -> Option<&'static str> {
    if outcome.compensates {
        return Some("a refusal that changes its row (`compensates: true`)");
    }
    if let Some(what) = affects(outcome) {
        return Some(what);
    }
    if outcome.subject.is_none() && outcome.instances.is_none() && !outcome.sets.is_empty() {
        return Some("`sets:` without a subject");
    }
    None
}

/// The secondary effect (`affects:`), named, when the kernel does not run it. The kernel runs an
/// entry that selects its rows by a filter beside a subject the input names and that moves,
/// updates or removes it ([`crate::execute`]). An entry naming its rows by the elements of an
/// input list (`each:`), a filter that walks the rows of an entity, and an entry beside any other
/// subject stay refused.
fn affects(outcome: &ResolvedOutcome) -> Option<&'static str> {
    if outcome.affects.is_empty() {
        return None;
    }
    let beside = outcome.instances.is_none()
        && outcome.subject.as_ref().is_some_and(|subject| {
            matches!(subject.instance, ResolvedInstance::Supplied { .. })
                && matches!(
                    subject.effect,
                    ResolvedEffect::Moves { .. }
                        | ResolvedEffect::Updates
                        | ResolvedEffect::Deletes
                )
        });
    if !beside {
        return Some(
            "secondary effects (`affects:`) beside no subject the input names that moves, \
             updates or removes",
        );
    }
    for affect in &outcome.affects {
        let Some(filter) = &affect.filter else {
            return Some("an `affects:` entry over an input list (`each:`)");
        };
        if !ess_domain::command::related_rows::walked(filter).is_empty() {
            return Some("a rows quantifier in an `affects:` filter");
        }
    }
    None
}

/// The outcome's shape, named, when the kernel does not run it: what selection reads of the rows
/// it acts on.
fn shape(outcome: &ResolvedOutcome) -> Option<&'static str> {
    if outcome.replays.is_some() || outcome.retains_result {
        return Some("a replayed or retained result");
    }
    if outcome.before_row {
        return Some("an external refusal before the row (`before_row: true`)");
    }
    // The rows a set outcome changes are selected from the store when it is taken
    // ([`crate::execute`]); a filter that walks the rows of an entity stays refused, as in a
    // row-set guard.
    if let Some(set) = &outcome.instances {
        return match set.effect {
            ResolvedEffect::Moves { .. } | ResolvedEffect::Updates | ResolvedEffect::Deletes
                if !ess_domain::command::related_rows::walked(&set.filter).is_empty() =>
            {
                Some("a rows quantifier in an `instances:` filter")
            }
            ResolvedEffect::Moves { .. } | ResolvedEffect::Updates | ResolvedEffect::Deletes => {
                None
            }
            ResolvedEffect::Creates | ResolvedEffect::Preserves => {
                Some("a set outcome (`instances:`) that neither moves, updates nor removes")
            }
        };
    }
    match &outcome.subject {
        None => None,
        Some(subject) => match (&subject.effect, &subject.instance) {
            (ResolvedEffect::Creates, ResolvedInstance::Observed { .. })
            | (
                ResolvedEffect::Moves { .. } | ResolvedEffect::Updates | ResolvedEffect::Deletes,
                ResolvedInstance::Supplied { .. },
            ) => None,
            (ResolvedEffect::Preserves, ResolvedInstance::Supplied { .. })
                if outcome.sets.is_empty() =>
            {
                None
            }
            (ResolvedEffect::Preserves, _) => Some("`preserves:` beside `sets:`"),
            (ResolvedEffect::Deletes, _) => Some("`deletes:` with an observed identity"),
            (ResolvedEffect::Creates, ResolvedInstance::Supplied { .. }) => {
                Some("`creates:` with a supplied identity")
            }
            (_, ResolvedInstance::Observed { .. }) => {
                Some("an observed identity on an existing row")
            }
        },
    }
}

/// The response field, named, whose value the runtime does not choose, where `outcome` answers
/// the command's response ([`crate::mint::answers`]). Each field is minted from its declared type
/// by the rule for generated fields, an `Optional` one as its required type; a one-time field
/// (`one_time_response:`) must be minted as text, which is fresh on every draw.
fn response(ir: &EssIr, command: &ResolvedCommand, outcome: &ResolvedOutcome) -> Option<String> {
    if !crate::mint::answers(outcome) {
        return None;
    }
    command.response.iter().find_map(|field| {
        let one_time = outcome.one_time_response.contains(&field.name);
        match crate::mint::response(ir, &field.type_ref, one_time) {
            Ok(_) => None,
            Err(crate::mint::Unanswered::OneTimeNotText) => Some(format!(
                "the command's one-time response field `{}` of type `{}`, which the runtime mints \
                 only as text",
                field.name, field.type_ref
            )),
            Err(crate::mint::Unanswered::Unchosen) => Some(format!(
                "the command's response field `{}` of type `{}`, which the runtime does not \
                 choose; {}",
                field.name,
                field.type_ref,
                crate::mint::CHOOSES
            )),
        }
    })
}

/// Where a determined field is read, as [`value`] asks.
#[derive(Clone, Copy)]
struct Reading {
    /// The outcome is a set outcome (`instances:`).
    set: bool,
    /// The field is an error's.
    error: bool,
    /// The field is an event payload's, of an outcome that answers the command's response.
    response: bool,
}

/// The first `{payload: <Event>.<field>}` of `outcome`, named, whose published value reads the
/// command's response, in `sets:`, `affects:`, a payload or an error's fields. ESS's interpreter
/// resolves every such read before it prepares the response (`interpret::execute::take` calls
/// `publish_reads` first), so it does not interpret one, and neither does the kernel.
fn published_response(outcome: &ResolvedOutcome) -> Option<String> {
    fn reads<'f>(fields: &'f [ResolvedPayloadField], out: &mut Vec<&'f ResolvedPayloadField>) {
        for field in fields {
            match &field.value {
                ResolvedPayloadValue::PayloadRead { .. } => out.push(field),
                ResolvedPayloadValue::Struct { fields } => reads(fields, out),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    reads(&outcome.sets, &mut found);
    for affect in &outcome.affects {
        reads(&affect.sets, &mut found);
    }
    for payload in &outcome.payload {
        reads(&payload.fields, &mut found);
    }
    reads(&outcome.error_payload, &mut found);
    found.into_iter().find_map(|field| {
        let ResolvedPayloadValue::PayloadRead {
            event, field: read, ..
        } = &field.value
        else {
            return None;
        };
        let answered = answered_read(outcome, event, read, 0)?;
        Some(format!(
            "`{}` stored from `{{payload: {}.{read}}}`, which reads the response field \
             `{answered}` before the outcome answers it",
            field.target,
            event.name()
        ))
    })
}

/// The response field the published `event`.`read` of `outcome` reaches, through its struct
/// members and the published values it reads in turn; `None` where it reads none. A chain longer
/// than [`ess_domain::types::MAX_TYPE_DEPTH`] reads none: ESS refuses a cycle of them.
fn answered_read<'o>(
    outcome: &'o ResolvedOutcome,
    event: &EventHandle,
    read: &str,
    depth: usize,
) -> Option<&'o str> {
    fn reaches<'o>(
        outcome: &'o ResolvedOutcome,
        value: &'o ResolvedPayloadValue,
        depth: usize,
    ) -> Option<&'o str> {
        match value {
            ResolvedPayloadValue::ResponseField { field, .. } => Some(field),
            ResolvedPayloadValue::Struct { fields } => fields
                .iter()
                .find_map(|member| reaches(outcome, &member.value, depth)),
            ResolvedPayloadValue::PayloadRead { event, field, .. } => {
                answered_read(outcome, event, field, depth + 1)
            }
            _ => None,
        }
    }
    if depth > ess_domain::types::MAX_TYPE_DEPTH {
        return None;
    }
    let source = outcome
        .payload
        .iter()
        .filter(|payload| payload.event == *event)
        .flat_map(|payload| &payload.fields)
        .find(|source| source.target == read)?;
    reaches(outcome, &source.value, depth)
}

/// Whether an event payload of `outcome` reads the command's response (`{response: <field>}`),
/// struct members included: the outcome whose response is prepared before its creation and its
/// events ([`crate::execute`]), as ESS's interpreter prepares it (`interpret::response::required`).
pub(crate) fn reads_response(outcome: &ResolvedOutcome) -> bool {
    fn reads(field: &ResolvedPayloadField) -> bool {
        match &field.value {
            ResolvedPayloadValue::ResponseField { .. } => true,
            ResolvedPayloadValue::Struct { fields } => fields.iter().any(reads),
            _ => false,
        }
    }
    outcome
        .payload
        .iter()
        .any(|payload| payload.fields.iter().any(reads))
}

/// The value source, named, when the kernel does not read it. A generated value is read where
/// its type is one the runtime mints ([`crate::mint::minted`]), and the refusal of one that is
/// not names the field and its type. What a set outcome changed (`{count: changed}`,
/// `{changed: …}`) is read where the outcome is a set outcome. A computed value (`{expr: …}`) is
/// read outside an error's fields, where ESS's interpreter computes none. A response field
/// (`{response: …}`) is read in an event payload of an outcome that answers the response, as
/// ESS's interpreter reads it (`interpret::execute::determined::response`); anywhere else ESS
/// has no response in hand.
fn value(ir: &EssIr, field: &ResolvedPayloadField, reading: Reading) -> Option<String> {
    let Reading { set, error, .. } = reading;
    let mintable = |present: bool| {
        (!crate::mint::absent(&field.target_type, present)
            && crate::mint::generated(ir, &field.target_type, present).is_none())
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
        | ResolvedPayloadValue::CallerAttribute { .. }
        | ResolvedPayloadValue::Cleared => None,
        ResolvedPayloadValue::ChangedCount
        | ResolvedPayloadValue::ChangedField { .. }
        | ResolvedPayloadValue::ChangedState { .. }
            if set =>
        {
            None
        }
        // Computed by ESS's own evaluator ([`crate::values`]); ESS's interpreter computes no
        // expression in an error's fields (`interpret::execute::error_value`), so neither does
        // the kernel.
        ResolvedPayloadValue::Expression { .. } if !error => None,
        ResolvedPayloadValue::ResponseField { .. } if reading.response => None,
        ResolvedPayloadValue::ResponseField { .. } => Some(format!(
            "the value source {} outside an event payload of an outcome that answers the \
             response",
            field.value.describe()
        )),
        ResolvedPayloadValue::Generated { present } => mintable(*present),
        ResolvedPayloadValue::InputOrGenerated {
            otherwise: None, ..
        } => mintable(false),
        ResolvedPayloadValue::Struct { fields } => {
            fields.iter().find_map(|member| value(ir, member, reading))
        }
        // The field or the held state of the row an input field names (ess/16, ess/28), read as
        // the store held it before the outcome ([`crate::values::related`]). A reference stored on
        // the subject, a chain through a further reference, and a list of references stay
        // refused.
        ResolvedPayloadValue::RelatedField {
            via: via @ ResolvedRelatedVia::Input { .. },
            through,
            ..
        }
        | ResolvedPayloadValue::RelatedState {
            via: via @ ResolvedRelatedVia::Input { .. },
            through,
            ..
        } if through.is_empty()
            && !matches!(via.type_ref().required(), ResolvedTypeRef::List { .. }) =>
        {
            None
        }
        other => Some(format!("the value source {}", other.describe())),
    }
}
