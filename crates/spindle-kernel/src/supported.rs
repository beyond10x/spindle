//! What the kernel runs, checked before a command runs.
//!
//! The kernel runs the conditions, effects and value sources Spindle's own specification and the
//! `rally` example use, a value read from the row an input field names, a value an expression
//! computes outside an error, a guard over a row set, a guard over a list of references for a
//! missing row, an external refusal asked before the row, a set outcome (`instances:`) with the
//! rows it changed, secondary effects (`affects:`) selecting their rows by a filter, and an event
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
//! beyond its subject (a compensating refusal, an `affects:` entry over an input list or over an
//! entity another component owns, `sets:` without a subject), a value it
//! writes, publishes or answers, the identity it mints, an error field it fills, or a value read
//! from a row of an entity the component running the command does not own. Selection
//! never reads one, so a request that selects another outcome of the command runs, and only a
//! request that selects this one is refused, once it is selected and before anything is minted
//! or written.
//!
//! A read of an entity another component owns (a value, a `when_related:` guard, a row set) is
//! refused only where the store the command runs on does not reach the owner's: [`takes`] and
//! [`check`] reach the running component's store alone, [`takes_reaching`] and [`check_reaching`]
//! the stores of the components they are given, as a host running several components in one
//! process reads each entity from its owner's store.

use std::collections::BTreeSet;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EntityHandle, EventHandle, ResolvedCommand, ResolvedCondition, ResolvedEffect, ResolvedEntity,
    ResolvedFallback, ResolvedInstance, ResolvedOutcome, ResolvedPayloadField,
    ResolvedPayloadValue, ResolvedRelatedTest, ResolvedRelatedVia, ResolvedTypeRef,
};

use crate::ExecuteError;

/// Refuses `command` when it or any of its outcomes declares a construct the kernel does not run,
/// outcome by outcome in declaration order: what [`selects`] refuses, and what [`takes`] refuses
/// for any outcome. A host asks it to know whether every request of the command runs.
///
/// # Errors
/// [`ExecuteError::Unsupported`], naming the construct and the outcome that declares it.
pub fn check(ir: &EssIr, command: &ResolvedCommand) -> Result<(), ExecuteError> {
    checking(ir, command, None)
}

/// [`check`], on a store that reaches the stores of the components `reached` names: a read of an
/// entity one of them owns is read from its owner's store, so it is not refused
/// ([`takes_reaching`]).
///
/// # Errors
/// As [`check`].
pub fn check_reaching(
    ir: &EssIr,
    command: &ResolvedCommand,
    reached: &BTreeSet<String>,
) -> Result<(), ExecuteError> {
    checking(ir, command, Some(reached))
}

/// [`check`], reaching the stores `reached` names, or the running component's alone.
fn checking(
    ir: &EssIr,
    command: &ResolvedCommand,
    reached: Option<&BTreeSet<String>>,
) -> Result<(), ExecuteError> {
    mixed(command)?;
    for outcome in &command.outcomes {
        selecting(command, outcome)?;
        taking(ir, command, outcome, reached)?;
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
    taking(ir, command, outcome, None)
}

/// [`takes`], on a store that reaches the stores of the components `reached` names, as a host
/// running them in one process hands the kernel: a value, a `when_related:` guard or a row set
/// over an entity one of them owns is read from its owner's store, as ESS's interpreter reads it
/// from its one store, so it is not refused. A read of an entity whose owner `reached` does not
/// name, or that no component owns, is refused as [`takes`] refuses it, and so is an `affects:`
/// entry over an entity the running component does not own, whatever is reached: the kernel
/// writes only the running component's store.
///
/// # Errors
/// As [`takes`].
pub fn takes_reaching(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
    reached: &BTreeSet<String>,
) -> Result<(), ExecuteError> {
    taking(ir, command, outcome, Some(reached))
}

/// [`takes`], reaching the stores `reached` names, or the running component's alone.
pub(crate) fn taking(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
    reached: Option<&BTreeSet<String>>,
) -> Result<(), ExecuteError> {
    let refuse = |what: &str| Err(unsupported(command, outcome, what));
    let set = outcome.instances.is_some();
    if let Some(what) = foreign_guard(ir, command, reached) {
        return refuse(&what);
    }
    if let Some(what) = effect(outcome) {
        return refuse(what);
    }
    if let Some(what) = foreign_affects(ir, command, outcome) {
        return refuse(&what);
    }
    if let Some(what) = response(ir, command, outcome) {
        return refuse(&what);
    }
    let minted = crate::mint::slot(ir, outcome);
    if let Some((entity, field)) = minted {
        let Some(shape) = crate::mint::identity(ir, &entity.identity.type_ref) else {
            return refuse(&format!(
                "a creation whose identity `{}.{}` of type `{}` neither the input nor the \
                 outcome determines; the runtime mints a `Uuid`, `String` or `Integer` identity, \
                 a `Uuid` or `Integer` newtype without invariants, or a `String` newtype whose \
                 prefix and alphabet admit a UUID's text and whose invariants bound only a \
                 length that text meets",
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
    if let Some(what) = foreign_read(ir, command, outcome, reached) {
        return refuse(&what);
    }
    Ok(())
}

/// The `affects:` entry, named, over an entity a component running `command` does not own. The
/// kernel selects and writes the rows of an `affects:` entry in the store it runs on, which a host
/// hands it to write the running component's rows: such an entry would select none of the
/// owner's rows and write rows the owner never reads, where ESS's interpreter, which holds every
/// entity in one store, changes the owner's. It is refused in every run, whatever store is
/// reached. A command no component accepts is not checked.
fn foreign_affects(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
) -> Option<String> {
    outcome.affects.iter().find_map(|affect| {
        let entity = ir.entity(&affect.entity);
        let (owner, runner) = foreign(ir, command, entity, None)?;
        Some(format!(
            "an `affects:` entry over `{}`, owned by {owner}: the kernel writes only the store \
             of component `{runner}`, which runs the command",
            entity.name,
        ))
    })
}

/// The value, named, that reads a row of an entity a component running `command` does not own:
/// `{related: {entity, where, field}}` (ess/22) or `{related: {via, field}}` (ess/16, ess/28), in
/// `outcome`'s `sets:`, `affects:`, payloads or error, struct members included. The kernel reads
/// only the store it runs on ([`crate::values`]); where that store does not reach the owner's
/// (`reached`, [`foreign`]), no row of such an entity is kept there, so the read would find none
/// and answer `no_value` for every request. ESS's interpreter holds every entity in one store and
/// reads it: the outcome is refused rather than answered otherwise. A command no component
/// accepts is not checked.
fn foreign_read(
    ir: &EssIr,
    command: &ResolvedCommand,
    outcome: &ResolvedOutcome,
    reached: Option<&BTreeSet<String>>,
) -> Option<String> {
    fn reads<'f>(
        fields: impl IntoIterator<Item = &'f ResolvedPayloadField>,
        found: &mut Vec<(&'f ResolvedPayloadField, &'f EntityHandle)>,
    ) {
        for field in fields {
            match &field.value {
                ResolvedPayloadValue::RelatedSelection { selection, .. } => {
                    found.push((field, &selection.entity));
                }
                ResolvedPayloadValue::RelatedField { entity, .. }
                | ResolvedPayloadValue::RelatedState { entity, .. } => found.push((field, entity)),
                ResolvedPayloadValue::Struct { fields } => reads(fields, found),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    reads(
        outcome
            .sets
            .iter()
            .chain(outcome.affects.iter().flat_map(|affect| &affect.sets))
            .chain(outcome.payload.iter().flat_map(|payload| &payload.fields))
            .chain(&outcome.error_payload),
        &mut found,
    );
    found.into_iter().find_map(|(field, entity)| {
        let entity = ir.entity(entity);
        let (owner, runner) = foreign(ir, command, entity, reached)?;
        Some(format!(
            "the value source {} for `{}`, which reads a row of `{}`, owned by {owner}: the \
             kernel reads only the store of component `{runner}`, which runs the command",
            field.value.describe(),
            field.target,
            entity.name,
        ))
    })
}

/// Where `entity` is owned by another component than one running `command`, in a store that does
/// not reach the owner's: the owner, named (`component \`<name>\`` or `no component`), and the
/// component running the command that does not own it, one `reached` names where one does.
/// `None` where every component accepting `command` owns it, where `reached` names the owner (a
/// host running both reads the entity from the owner's store), and for a command no component
/// accepts. With `reached` `None` only the running component's store is reached.
fn foreign(
    ir: &EssIr,
    command: &ResolvedCommand,
    entity: &ResolvedEntity,
    reached: Option<&BTreeSet<String>>,
) -> Option<(String, String)> {
    let name = command.name.to_string();
    let components = ir.components().values();
    let mut runners = components.clone().filter(|component| {
        component
            .accepts
            .iter()
            .any(|accepted| accepted.to_string() == name)
            && !component.owns.contains(&entity.domain)
    });
    let runner = match reached {
        Some(reached) => runners
            .clone()
            .find(|runner| reached.contains(runner.name.as_str()))
            .or_else(|| runners.next()),
        None => runners.next(),
    }?;
    let owner = components
        .clone()
        .find(|component| component.owns.contains(&entity.domain));
    if let (Some(owner), Some(reached)) = (owner, reached)
        && reached.contains(owner.name.as_str())
    {
        return None;
    }
    let owner = owner.map_or_else(
        || "no component".to_owned(),
        |owner| format!("component `{}`", owner.name.as_str()),
    );
    Some((owner, runner.name.as_str().to_owned()))
}

/// The guard, named, of any outcome of `command` that reads the rows of an entity a component
/// running `command` does not own: a `when_related:` row set over such an entity, a rows
/// quantifier in its selector or test walking one (`exists: {in: {entity: …}, …}`, ess/27), or a
/// `when_related:` row such an entity holds. The kernel reads only the store it runs on; where that
/// store does not reach the owner's (`reached`, [`foreign`]), no row of such an entity is kept
/// there, so the guard would be decided over no rows; ESS's interpreter holds every entity in one
/// store and reads them. Selection reads every guard it reaches, so every outcome of the command
/// is refused, as [`foreign_read`] refuses a value. A command no component accepts is not checked.
fn foreign_guard(
    ir: &EssIr,
    command: &ResolvedCommand,
    reached: Option<&BTreeSet<String>>,
) -> Option<String> {
    command.outcomes.iter().find_map(|outcome| {
        let read: Vec<&ResolvedEntity> = match &outcome.condition {
            ResolvedCondition::RelatedSet {
                selection, test, ..
            } => std::iter::once(ir.entity(&selection.entity))
                .chain(
                    std::iter::once(&selection.filter)
                        .chain(test.predicate())
                        .flat_map(ess_domain::command::related_rows::walked)
                        .filter_map(|name| ir.entities().get(&name.parse().ok()?)),
                )
                .collect(),
            ResolvedCondition::Related { entity, also, .. } => std::iter::once(entity)
                .chain(also.iter().map(|also| &also.entity))
                .map(|entity| ir.entity(entity))
                .collect(),
            _ => Vec::new(),
        };
        read.into_iter().find_map(|entity| {
            let (owner, runner) = foreign(ir, command, entity, reached)?;
            Some(format!(
                "the `when_related:` guard of `{}/{}`, which reads the rows of `{}`, owned by \
                 {owner}: the kernel reads only the store of component `{runner}`, which runs \
                 the command",
                command.name,
                outcome.name.as_str(),
                entity.name,
            ))
        })
    })
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
        // A list of references the input names is read for a missing row alone
        // ([`crate::execute`]), as ESS reads it (ess/28); a predicate over its rows and a stored
        // list stay refused, as ESS refuses them.
        ResolvedCondition::Related {
            via: ResolvedRelatedVia::Input { .. },
            test: ResolvedRelatedTest::Absent,
            ..
        } => None,
        ResolvedCondition::Related { via, .. }
            if matches!(via.type_ref(), ResolvedTypeRef::List { .. }) =>
        {
            Some("`when_related:` over a list of references other than `exists: false` on an input")
        }
        ResolvedCondition::Related { .. } => None,
        // A row set is read from the store before selection, with the rows of every entity its
        // selector or `forall:` walks (`exists: {in: {entity: …}, …}`, ess/27) ([`crate::execute`]).
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
    // An external refusal answered before the row (`before_row: true`) is asked among the input
    // refusals, before the addressed row is read ([`crate::execute`]).
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
/// has no response in hand. An input with a literal or another input after `else:` is read
/// wherever ESS's interpreter reads one: everywhere but an error's fields, and there over an
/// input path or with another input after `else:`.
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
        // The first element of a list of references that names no row, read from the rows the
        // list names as the store held them before the command ran ([`crate::values::related`]).
        | ResolvedPayloadValue::MissingElement { .. }
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
        // The input, else the literal or the other input `else:` names, as ESS's interpreter
        // reads them (`interpret::execute::value`); nothing is minted. In an error's fields ESS
        // reads one only over an input path or with another input after `else:`
        // (`interpret::execute::error_value`), so a literal after a plain input stays refused
        // there.
        ResolvedPayloadValue::InputOrGenerated {
            field: read,
            otherwise: Some(otherwise),
            ..
        } if !error
            || ess_domain::command::input_path::is_path(read)
            || matches!(otherwise, ResolvedFallback::Input { .. }) =>
        {
            None
        }
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
        // A field of the one row a selector selects (ess/22), read from the store as it was
        // before the outcome ([`crate::values::Selections`]). A selector that reads what its own
        // outcome publishes (`payload.<Event>.<field>`, ess/27) or walks the rows of an entity
        // stays refused, and so does one in an error's fields, where ESS's interpreter reads none
        // (`interpret::execute::error_value`).
        ResolvedPayloadValue::RelatedSelection { selection, .. }
            if !error
                && selection.reads.is_empty()
                && ess_domain::command::related_rows::walked(&selection.filter).is_empty() =>
        {
            None
        }
        other => Some(format!("the value source {}", other.describe())),
    }
}
