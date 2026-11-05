//! One command, from input to committed change.

use std::collections::BTreeMap;
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EventHandle, PrecedencePlan, ResolvedCommand, ResolvedCondition, ResolvedEffect,
    ResolvedEntity, ResolvedField, ResolvedInstance, ResolvedOutcome, ResolvedPayloadField,
    ResolvedPayloadValue,
};
use ess_domain::QualifiedName;
use ess_domain::command::precedence::Phase;
use ess_primitives::predicate::{Predicate, Truth};
use serde_json::{Map, Value};

use crate::decider::{Decider, External};
use crate::facts::Facts;
use crate::mint::{Generator, IDENTITY_MINT, Identity, Mint, UuidV4};
use crate::store::{Change, Row, Store, Stored};
use crate::values::Sources;
use crate::{DeclaredError, Event, ExecuteError, Executed};

/// The prefix of the capability an `external:` outcome is answered through, as admission derives
/// it.
const EXTERNAL: &str = "external.";

/// Checks `row`, held as the instance of `entity` with `identity`, against every invariant the
/// entity declares, as a command's change is checked before it is committed.
///
/// For a row that reaches a store without a command: a seed, an import, a test's arrangement.
/// An invariant the row leaves `Unknown` concludes nothing.
///
/// # Errors
/// [`ExecuteError::BrokenInvariant`] for the first declared invariant the row breaks, and
/// [`ExecuteError::InvalidInput`] when the model declares no entity `entity`.
pub fn at_rest(ir: &EssIr, entity: &str, identity: &Value, row: &Row) -> Result<(), ExecuteError> {
    let resolved = ir
        .entities()
        .iter()
        .find_map(|(name, resolved)| (name.to_string() == entity).then_some(resolved))
        .ok_or_else(|| {
            ExecuteError::InvalidInput(format!("the model declares no entity {entity}"))
        })?;
    rests(ir, resolved, identity, row)
}

/// [`at_rest`] for a resolved entity.
fn rests(
    ir: &EssIr,
    entity: &ResolvedEntity,
    identity: &Value,
    row: &Row,
) -> Result<(), ExecuteError> {
    let facts = Facts::row(ir, entity, identity, row, None);
    for invariant in &entity.invariants {
        if invariant.predicate.evaluate(&facts) == Truth::False {
            return Err(ExecuteError::BrokenInvariant {
                instance: format!("`{}` {}", entity.name, identity),
                invariant: invariant.statement.clone(),
            });
        }
    }
    Ok(())
}

/// Executes `command` with `input` against the instances in `store`.
///
/// Reads the instance the command addresses, selects the outcome, and commits that outcome's
/// change in one [`Store::commit`]. A refusal, a `preserves:` outcome and a `refuses: false`
/// outcome commit nothing. An identity the runtime mints is a random UUIDv4 ([`UuidV4`]).
///
/// # Errors
/// [`ExecuteError`] where the command does not run; the store is then unchanged.
pub fn execute(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    command: &str,
    input: &Map<String, Value>,
) -> Result<Executed, ExecuteError> {
    execute_minting(ir, store, decider, &mut UuidV4, command, input)
}

/// [`execute`], minting every identity the runtime mints through `mint`.
///
/// A `creates:` outcome whose identity is neither an input nor a payload source asks `mint` once,
/// and the value is the new instance's identity and every event payload field of the identity's
/// name.
///
/// A field marked `{generated: true}` other than that identity is minted from its type through
/// `mint` too; a generated `Timestamp` is the machine's current time ([`execute_at`] takes a
/// clock instead).
///
/// # Errors
/// As [`execute`].
pub fn execute_minting(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    command: &str,
    input: &Map<String, Value>,
) -> Result<Executed, ExecuteError> {
    execute_at(ir, store, decider, mint, &SystemTime::now, command, input)
}

/// [`execute_minting`], stamping every generated `Timestamp` with the civil time `now` answers:
/// the clock an environment binds as `clock`.
///
/// # Errors
/// As [`execute`].
pub fn execute_at(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    now: &dyn Fn() -> SystemTime,
    command: &str,
    input: &Map<String, Value>,
) -> Result<Executed, ExecuteError> {
    execute_as(
        ir,
        store,
        decider,
        mint,
        now,
        &Invocation {
            command,
            input,
            actor: None,
            caller: None,
        },
    )
}

/// One command as it is sent: its input, and the attributes of the caller it is sent as.
///
/// The caller is separate from the input (ESS `interpret::execute::caller`): a command reads it
/// as `{caller: <attribute>}` and as `caller.<attribute>` in a guard, never as an input field.
/// `None` is a command sent as no caller, as every command a binding or a tick invokes is.
#[derive(Debug, Clone, Copy)]
pub struct Invocation<'a> {
    /// The command's qualified name.
    pub command: &'a str,
    /// Its input, by declared field name.
    pub input: &'a Map<String, Value>,
    /// The qualified name of the actor it is sent as, where it is sent as one.
    pub actor: Option<&'a str>,
    /// The caller's attributes, by declared attribute name.
    pub caller: Option<&'a Map<String, Value>>,
}

/// [`execute_at`] for an [`Invocation`], sent as its caller.
///
/// The caller is bound against the attributes of the actor it is sent as, as ESS binds it, before
/// anything is read: attributes sent as no declared actor, an attribute the actor does not
/// declare, a value its type does not admit and a required attribute left out are each
/// [`ExecuteError::InvalidInput`]. A command sent as no caller that reads a required attribute is
/// [`ExecuteError::InvalidInput`] where it reads it as a value, and a guard over one is `Unknown`
/// ([`ExecuteError::Undecidable`]); either way nothing is written.
///
/// # Errors
/// As [`execute`].
pub fn execute_as(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    now: &dyn Fn() -> SystemTime,
    invocation: &Invocation<'_>,
) -> Result<Executed, ExecuteError> {
    let Invocation {
        command,
        input,
        actor,
        caller,
    } = *invocation;
    let spec = ir
        .commands()
        .iter()
        .find_map(|(name, spec)| (name.to_string() == command).then_some(spec))
        .ok_or_else(|| ExecuteError::UnknownCommand(command.to_owned()))?;
    crate::supported::check(ir, spec)?;
    let attributes = crate::caller::bind(ir, actor, caller)?;
    check_input(ir, spec, input)?;
    let addressed = addressed(ir, spec)?;
    let held = match addressed {
        Some((entity, field)) => {
            let identity = input
                .get(&field.name)
                .filter(|identity| !identity.is_null())
                .ok_or_else(|| {
                    ExecuteError::InvalidInput(format!("`{}` names no instance", field.name))
                })?
                .clone();
            let stored = store.load(&entity.name.to_string(), &identity)?;
            Some(Addressed {
                entity,
                identity,
                stored,
            })
        }
        None => None,
    };
    let mut run = Run {
        ir,
        spec,
        command,
        input,
        caller,
        attributes,
        store,
        decider,
        held,
        generator: Generator::new(mint, now, command),
        minted: None,
    };
    match run.select()? {
        Selected::Outcome(outcome) => run.take(outcome),
        Selected::Answered(executed) => Ok(executed),
    }
}

/// The instance the command addresses, as read before deciding.
struct Addressed<'ir> {
    entity: &'ir ResolvedEntity,
    identity: Value,
    stored: Option<Stored>,
}

/// What selection found.
enum Selected<'ir> {
    /// The outcome to take.
    Outcome(&'ir ResolvedOutcome),
    /// A refusal already answered for the instance.
    Answered(Executed),
}

struct Run<'a, 'ir> {
    ir: &'ir EssIr,
    spec: &'ir ResolvedCommand,
    command: &'a str,
    input: &'a Map<String, Value>,
    /// The attributes of the caller the command is sent as; `None` for no caller.
    caller: Option<&'a Map<String, Value>>,
    /// The attributes the actor the command is sent as declares.
    attributes: &'ir [ResolvedField],
    store: &'a mut dyn Store,
    decider: &'a mut dyn Decider,
    held: Option<Addressed<'ir>>,
    generator: Generator<'a>,
    /// The identity this run minted: the creating event that publishes it, and the identity
    /// field's name.
    minted: Option<(&'ir EventHandle, String, Value)>,
}

impl<'a, 'ir> Run<'a, 'ir> {
    /// Reads the outcomes in the order of the command's precedence plan; the first whose
    /// condition holds is selected.
    fn select(&mut self) -> Result<Selected<'ir>, ExecuteError> {
        let plan = PrecedencePlan::new(self.spec, self.ir.format());
        for planned in plan.phases() {
            match planned.phase {
                Phase::Existence => {
                    let Some(addressed) = &self.held else {
                        continue;
                    };
                    if addressed.stored.is_some() {
                        continue;
                    }
                    if let Some(outcome) = planned
                        .branches
                        .iter()
                        .find(|outcome| outcome.condition == ResolvedCondition::UnknownInstance)
                    {
                        return Ok(Selected::Outcome(outcome));
                    }
                    // An instance nobody holds rests in no state any move of this command starts
                    // from: the wrong-state answer, where the command declares one.
                    return self.wrong_state().map(Selected::Answered);
                }
                Phase::InputRefusal
                | Phase::HeldState
                | Phase::PresentRelated
                | Phase::Accepting
                | Phase::Default => {
                    for outcome in &planned.branches {
                        if self.holds(outcome)? {
                            return Ok(Selected::Outcome(outcome));
                        }
                    }
                }
                Phase::InputAbsent | Phase::RelatedRow => {
                    if let Some(outcome) = planned.branches.first() {
                        return Err(ExecuteError::Unsupported {
                            construct: format!(
                                "the `{}` step, in `{}/{}`",
                                planned.phase,
                                self.command,
                                outcome.name.as_str()
                            ),
                        });
                    }
                }
            }
        }
        Err(ExecuteError::Undeclared {
            command: self.command.to_owned(),
            reason: "no outcome's condition holds and the command declares no default".to_owned(),
        })
    }

    /// Whether `outcome`'s condition holds for the held instance and the input.
    ///
    /// A condition with several parts reads them in the order ESS lowers them: the held instance,
    /// then the input guard, then the external cause. The decider is asked only once every other
    /// part holds, so a request the model already turns away never reaches it.
    fn holds(&mut self, outcome: &ResolvedOutcome) -> Result<bool, ExecuteError> {
        let held = match &outcome.condition {
            ResolvedCondition::Otherwise | ResolvedCondition::External { .. } => Truth::True,
            ResolvedCondition::When { predicate }
            | ResolvedCondition::ExternalWhen { predicate, .. } => self.guard(Some(predicate)),
            ResolvedCondition::SubjectPredicate { predicate, input }
            | ResolvedCondition::ExternalSubject {
                predicate, input, ..
            } => {
                let subject = predicate.evaluate(&self.subject_facts(outcome)?);
                both(subject, || self.guard(input.as_ref()))
            }
            ResolvedCondition::SubjectField {
                field,
                equals,
                predicate,
            } => {
                let row = self.held_row(outcome)?;
                let subject = match row.fields.get(field) {
                    None | Some(Value::Null) => Truth::Unknown,
                    Some(value) => truth(value.as_str() == Some(equals.as_str())),
                };
                both(subject, || self.guard(predicate.as_ref()))
            }
            ResolvedCondition::SubjectState { state, predicate } => {
                let row = self.held_row(outcome)?;
                let subject = truth(state.iter().any(|held| held.as_str() == row.state));
                both(subject, || self.guard(predicate.as_ref()))
            }
            // A marker, never selected by reading: the wrong-state answer comes from the branch that
            // is selected, when its move does not start from the state held (ESS `act`).
            ResolvedCondition::WrongState => Truth::False,
            other => {
                return Err(ExecuteError::Unsupported {
                    construct: format!(
                        "the condition {other:?} at this step, in `{}/{}`",
                        self.command,
                        outcome.name.as_str()
                    ),
                });
            }
        };
        match held {
            Truth::True => {}
            Truth::False => return Ok(false),
            Truth::Unknown => {
                return Err(ExecuteError::Undecidable {
                    outcome: format!("{}/{}", self.command, outcome.name.as_str()),
                });
            }
        }
        match &outcome.condition {
            ResolvedCondition::External { cause }
            | ResolvedCondition::ExternalWhen { cause, .. }
            | ResolvedCondition::ExternalSubject { cause, .. } => self.ask(outcome, cause),
            _ => Ok(true),
        }
    }

    /// An input guard over the command's input, read at the input fields' own names as `when:`
    /// reads them, and the caller under `caller.`; no guard holds.
    fn guard(&self, predicate: Option<&Predicate>) -> Truth {
        predicate.map_or(Truth::True, |predicate| {
            predicate.evaluate(
                &Facts::values(self.ir, &self.spec.input, self.input)
                    .with_caller(self.attributes, self.caller),
            )
        })
    }

    /// The facts a guard over the subject reads: the held instance, the input under `input.` and
    /// the caller under `caller.`, each unless the entity declares a field of that name, which
    /// keeps being read as itself.
    fn subject_facts(&self, outcome: &ResolvedOutcome) -> Result<Facts<'_>, ExecuteError> {
        let row = self.held_row(outcome)?;
        let Some(held) = &self.held else {
            return Err(self.no_subject(outcome));
        };
        let shadowed = held
            .entity
            .fields
            .iter()
            .any(|field| field.name == crate::facts::INPUT);
        let input = (!shadowed).then_some((self.spec.input.as_slice(), self.input));
        Ok(Facts::row(self.ir, held.entity, &held.identity, row, input)
            .with_caller(self.attributes, self.caller))
    }

    /// The held instance a guard over the subject reads.
    fn held_row(&self, outcome: &ResolvedOutcome) -> Result<&Row, ExecuteError> {
        self.before().ok_or_else(|| self.no_subject(outcome))
    }

    fn no_subject(&self, outcome: &ResolvedOutcome) -> ExecuteError {
        ExecuteError::Unsupported {
            construct: format!(
                "a guard over the subject with no addressed instance, in `{}/{}`",
                self.command,
                outcome.name.as_str()
            ),
        }
    }

    /// Whether an external cause holds, as the decider answers it.
    fn ask(&mut self, outcome: &ResolvedOutcome, cause: &str) -> Result<bool, ExecuteError> {
        let capability = format!("{EXTERNAL}{}.{}", self.command, outcome.name.as_str());
        let question = External {
            capability: capability.clone(),
            command: self.command,
            outcome: outcome.name.as_str(),
            cause,
            input: self.input,
        };
        self.decider
            .decide(&question)
            .map_err(|reason| ExecuteError::Undecided { capability, reason })
    }

    /// The command's `wrong_state:` outcome, answered for the addressed instance: its error, or
    /// with `refuses: false` an accepted request that changes nothing.
    fn wrong_state(&self) -> Result<Executed, ExecuteError> {
        let Some(outcome) = self
            .spec
            .outcomes
            .iter()
            .find(|outcome| outcome.condition == ResolvedCondition::WrongState)
        else {
            let reason = match &self.held {
                Some(Addressed {
                    entity,
                    identity,
                    stored: None,
                }) => format!(
                    "no `{}` has the identity {identity}, and the command declares neither \
                     `unknown_instance:` nor `wrong_state:`",
                    entity.name
                ),
                _ => "the held state is not one this command acts from, and the command \
                      declares no `wrong_state:`"
                    .to_owned(),
            };
            return Err(ExecuteError::Undeclared {
                command: self.command.to_owned(),
                reason,
            });
        };
        self.refusal(outcome)
    }

    /// An outcome answered for the instance rather than acting on it: it moves, writes and emits
    /// nothing, and reports its error where it refuses.
    fn refusal(&self, outcome: &ResolvedOutcome) -> Result<Executed, ExecuteError> {
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events: Vec::new(),
            error: self.declared_error(outcome, self.before())?,
        })
    }

    fn before(&self) -> Option<&Row> {
        self.held
            .as_ref()
            .and_then(|held| held.stored.as_ref())
            .map(|stored| &stored.row)
    }

    /// Applies `outcome`'s effect and commits it.
    fn take(&mut self, outcome: &'ir ResolvedOutcome) -> Result<Executed, ExecuteError> {
        let Some(subject) = &outcome.subject else {
            self.publish_reads(outcome, None)?;
            let before = self.before();
            return Ok(Executed {
                outcome: outcome.name.as_str().to_owned(),
                events: self.emit(outcome, before)?,
                error: self.declared_error(outcome, before)?,
            });
        };
        let entity = self.ir.entity(&subject.entity);
        let (change, events) = match (&subject.effect, &subject.instance) {
            (ResolvedEffect::Creates, ResolvedInstance::Observed { event, field }) => {
                self.publish_reads(outcome, None)?;
                if let Some((created, named)) = crate::mint::slot(self.ir, outcome) {
                    let minted = self.mint_identity(outcome, created, named)?;
                    self.minted = Some((event, named.name.clone(), minted));
                }
                let identity = if let Some((_, _, minted)) = &self.minted {
                    minted.clone()
                } else {
                    self.determined_identity(outcome, event, field)?
                };
                if self
                    .store
                    .load(&entity.name.to_string(), &identity)?
                    .is_some()
                {
                    return Err(ExecuteError::Undeclared {
                        command: self.command.to_owned(),
                        reason: format!(
                            "a `{}` already has the identity {identity}, and `creates:` never \
                             replaces an instance",
                            entity.name
                        ),
                    });
                }
                let state = subject
                    .into
                    .as_ref()
                    .unwrap_or(&entity.lifecycle.initial)
                    .to_string();
                let mut fields = BTreeMap::new();
                write(
                    &self.sources(outcome, &entity.name, None, None),
                    outcome,
                    &mut fields,
                )?;
                let events = self.emit(outcome, None)?;
                (
                    Some(Change {
                        entity: entity.name.to_string(),
                        identity,
                        expected: None,
                        row: Row { state, fields },
                        events: events.clone(),
                    }),
                    events,
                )
            }
            (effect, ResolvedInstance::Supplied { .. }) => {
                let Some(Addressed {
                    identity,
                    stored: Some(stored),
                    ..
                }) = &self.held
                else {
                    return self.wrong_state();
                };
                let mut after = stored.row.clone();
                if let ResolvedEffect::Moves { transition } = effect {
                    if !transition
                        .from
                        .iter()
                        .any(|state| state.as_str() == stored.row.state)
                    {
                        return self.wrong_state();
                    }
                    after.state = transition.to.to_string();
                }
                self.publish_reads(outcome, Some(&stored.row))?;
                let sources = self.sources(outcome, &entity.name, None, Some(&stored.row));
                write(&sources, outcome, &mut after.fields)?;
                let events = self.emit(outcome, Some(&stored.row))?;
                let change = (!matches!(effect, ResolvedEffect::Preserves)).then(|| Change {
                    entity: entity.name.to_string(),
                    identity: identity.clone(),
                    expected: Some(stored.version),
                    row: after,
                    events: events.clone(),
                });
                (change, events)
            }
            (effect, _) => {
                return Err(ExecuteError::Unsupported {
                    construct: format!(
                        "`{}` with this identity, in `{}/{}`",
                        effect.verb(),
                        self.command,
                        outcome.name.as_str()
                    ),
                });
            }
        };
        let error = self.declared_error(outcome, self.before())?;
        if let Some(change) = &change {
            self.at_rest(entity, change)?;
            self.store.commit(change)?;
        }
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events,
            error,
        })
    }

    /// Mints, before anything else the outcome draws, each published value a
    /// `{payload: <Event>.<field>}` reads: those `sets:` stores, then those the payloads read, in
    /// the order written. ESS's interpreter resolves them first (`interpret::execute::take` calls
    /// `publish_reads` before `create` mints the identity and before the other `sets:`), so a
    /// [`Mint`] that answers its tick UUIDs draws each value on the interpreter's tick. Each value
    /// is held by the [`Generator`], so the later read and the event see it.
    ///
    /// A read of the created identity is left to the creation, as before: ESS does not interpret
    /// a stored copy of it, and the kernel reads it from the identity it mints.
    fn publish_reads(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
    ) -> Result<(), ExecuteError> {
        let identity = outcome.subject.as_ref().and_then(|subject| {
            match (&subject.effect, &subject.instance) {
                (ResolvedEffect::Creates, ResolvedInstance::Observed { event, field }) => {
                    Some((event, field.name.as_str()))
                }
                _ => None,
            }
        });
        let mut reads = Vec::new();
        payload_reads(&outcome.sets, &mut reads);
        for payload in &outcome.payload {
            payload_reads(&payload.fields, &mut reads);
        }
        for (event, read) in reads {
            if identity.is_some_and(|(creating, name)| creating == event && name == read) {
                continue;
            }
            let Some(source) = outcome
                .payload
                .iter()
                .filter(|payload| payload.event == *event)
                .flat_map(|payload| &payload.fields)
                .find(|source| source.target == read)
            else {
                continue;
            };
            self.sources(outcome, event.name(), Some(event), before)
                .value(source)?;
        }
        Ok(())
    }

    /// The identity a creation's payload determines for the new instance, published in `field` of
    /// `event`.
    fn determined_identity(
        &self,
        outcome: &ResolvedOutcome,
        event: &EventHandle,
        field: &ResolvedField,
    ) -> Result<Value, ExecuteError> {
        outcome
            .payload
            .iter()
            .filter(|payload| payload.event == *event)
            .flat_map(|payload| &payload.fields)
            .find(|source| source.target == field.name)
            .map(|source| {
                self.sources(outcome, event.name(), Some(event), None)
                    .value(source)
            })
            .transpose()?
            .flatten()
            .ok_or_else(|| ExecuteError::Unsupported {
                construct: format!(
                    "a creation whose identity `{}.{}` the outcome does not determine, in `{}/{}`",
                    event.name(),
                    field.name,
                    self.command,
                    outcome.name.as_str()
                ),
            })
    }

    /// Asks the mint for the identity of the instance `outcome` creates, as the identity's type
    /// spells it, and refuses one that is not a value of that type.
    fn mint_identity(
        &mut self,
        outcome: &ResolvedOutcome,
        entity: &ResolvedEntity,
        field: &ResolvedField,
    ) -> Result<Value, ExecuteError> {
        let entity_name = entity.name.to_string();
        let refused = |error: String| {
            ExecuteError::InvalidValue(format!(
                "the identity minted for `{entity_name}.{}` in `{}/{}` would hold {error}",
                field.name,
                self.command,
                outcome.name.as_str()
            ))
        };
        let shape = crate::mint::identity(self.ir, &entity.identity.type_ref).ok_or_else(|| {
            ExecuteError::Unsupported {
                construct: format!(
                    "a creation whose identity `{entity_name}.{}` of type `{}` the runtime does \
                     not mint, in `{}/{}`",
                    field.name,
                    entity.identity.type_ref,
                    self.command,
                    outcome.name.as_str()
                ),
            }
        })?;
        let minted = self
            .generator
            .mint(
                &shape,
                &Identity {
                    capability: IDENTITY_MINT,
                    command: self.command,
                    outcome: outcome.name.as_str(),
                    entity: &entity_name,
                    field: &field.name,
                },
            )
            .map_err(&refused)?;
        crate::typed::validate(self.ir, &entity.identity.type_ref, &minted).map_err(&refused)?;
        Ok(minted)
    }

    /// Refuses a change that leaves its instance breaking an invariant its entity declares, as
    /// ESS's `at_rest` does. An invariant the row leaves `Unknown`, reading a field no outcome has
    /// written, concludes nothing.
    fn at_rest(&self, entity: &ResolvedEntity, change: &Change) -> Result<(), ExecuteError> {
        rests(self.ir, entity, &change.identity, &change.row)
    }

    /// What the fields of `owner` in `outcome` read, with the instance as it was `before`;
    /// `filling` is the event whose top-level payload fields these are, where they are one's.
    fn sources<'s>(
        &'s self,
        outcome: &'s ResolvedOutcome,
        owner: &'s QualifiedName,
        filling: Option<&EventHandle>,
        before: Option<&'s Row>,
    ) -> Sources<'s, 'a> {
        Sources {
            ir: self.ir,
            input: self.input,
            caller: self.caller,
            before,
            error: false,
            minted: self
                .minted
                .as_ref()
                .filter(|(creating, _, _)| filling.is_some_and(|filling| filling == *creating))
                .map(|(_, name, value)| (name.as_str(), value)),
            creation: self
                .minted
                .as_ref()
                .map(|(creating, name, value)| (*creating, name.as_str(), value)),
            outcome,
            owner,
            generator: &self.generator,
        }
    }

    /// The events `outcome` emits, each with the payload fields it determines.
    fn emit(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
    ) -> Result<Vec<Event>, ExecuteError> {
        outcome
            .emits
            .iter()
            .map(|event| {
                let fields = outcome
                    .payload
                    .iter()
                    .find(|payload| payload.event == *event)
                    .map_or(&[][..], |payload| payload.fields.as_slice());
                let mut payload = self
                    .sources(outcome, event.name(), Some(event), before)
                    .object(fields)?;
                // A minted identity is published in the field `instance:` names on the event
                // `instance:` names, where the outcome gives that field no source of its own,
                // and on no other event (ESS `interpret::execute::emit`).
                if let Some((creating, name, minted)) = &self.minted
                    && *creating == event
                    && !fields.iter().any(|source| source.target == *name)
                    && self.ir.events().get(event.name()).is_some_and(|declared| {
                        declared.fields.iter().any(|field| field.name == *name)
                    })
                {
                    payload.insert(name.clone(), minted.clone());
                }
                Ok(Event {
                    name: event.name().to_string(),
                    payload,
                })
            })
            .collect()
    }

    fn declared_error(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
    ) -> Result<Option<DeclaredError>, ExecuteError> {
        // A `wrong_state:` branch that accepts (`refuses: false`) reports no error.
        let Some(error) = outcome.error.as_ref().filter(|_| outcome.refuses) else {
            return Ok(None);
        };
        let sources = Sources {
            error: true,
            ..self.sources(outcome, error.name(), None, before)
        };
        Ok(Some(DeclaredError {
            name: error.name().to_string(),
            fields: sources.object(&outcome.error_payload)?,
        }))
    }
}

fn truth(holds: bool) -> Truth {
    if holds { Truth::True } else { Truth::False }
}

/// Both parts, in three-valued logic: one part that fails fails the whole, whatever the other.
/// `second` is read only where `first` leaves the answer open.
fn both(first: Truth, second: impl FnOnce() -> Truth) -> Truth {
    match (first, second) {
        (Truth::False, _) => Truth::False,
        (Truth::True, second) => second(),
        (Truth::Unknown, second) => match second() {
            Truth::False => Truth::False,
            _ => Truth::Unknown,
        },
    }
}

/// Every `{payload: <Event>.<field>}` in `fields`, struct members included, in the order written
/// (ESS `interpret::execute::reads_of`).
fn payload_reads<'f>(
    fields: &'f [ResolvedPayloadField],
    out: &mut Vec<(&'f EventHandle, &'f str)>,
) {
    for field in fields {
        match &field.value {
            ResolvedPayloadValue::PayloadRead {
                event, field: read, ..
            } => {
                out.push((event, read.as_str()));
            }
            ResolvedPayloadValue::Struct { fields } => payload_reads(fields, out),
            _ => {}
        }
    }
}

/// Writes `outcome`'s `sets:` into `fields`, reading the instance as it was before. Each value is
/// checked against the type its target declares, as ESS checks a sum or a written value.
fn write(
    sources: &Sources<'_, '_>,
    outcome: &ResolvedOutcome,
    fields: &mut BTreeMap<String, Value>,
) -> Result<(), ExecuteError> {
    let mut written = Vec::with_capacity(outcome.sets.len());
    for set in &outcome.sets {
        let value = sources.value(set)?;
        if let Some(value) = &value {
            crate::typed::validate(sources.ir, &set.target_type, value).map_err(|error| {
                ExecuteError::InvalidValue(format!("`{}` would hold {error}", set.target))
            })?;
        }
        written.push((set.target.clone(), value));
    }
    for (target, value) in written {
        match value {
            Some(value) => {
                fields.insert(target, value);
            }
            None => {
                fields.remove(&target);
            }
        }
    }
    Ok(())
}

/// The entity and the input field naming the instance the command's outcomes act on, where they
/// act on an existing one. Every outcome that names its instance in the input names the same.
fn addressed<'ir>(
    ir: &'ir EssIr,
    spec: &'ir ResolvedCommand,
) -> Result<Option<(&'ir ResolvedEntity, &'ir ResolvedField)>, ExecuteError> {
    let mut found: Option<(&ResolvedEntity, &ResolvedField)> = None;
    for outcome in &spec.outcomes {
        let Some(subject) = &outcome.subject else {
            continue;
        };
        let ResolvedInstance::Supplied { field } = &subject.instance else {
            continue;
        };
        let entity = ir.entity(&subject.entity);
        match found {
            None => found = Some((entity, field)),
            Some((first, named)) if first.name == entity.name && named.name == field.name => {}
            Some(_) => {
                return Err(ExecuteError::Unsupported {
                    construct: format!(
                        "outcomes of `{}` addressing different instances",
                        spec.name
                    ),
                });
            }
        }
    }
    Ok(found)
}

/// Refuses an input that is not a value of the command's declared input.
fn check_input(
    ir: &EssIr,
    spec: &ResolvedCommand,
    input: &Map<String, Value>,
) -> Result<(), ExecuteError> {
    for name in input.keys() {
        if spec.input_field(name).is_none() {
            return Err(ExecuteError::InvalidInput(format!(
                "`{}` declares no input `{name}`",
                spec.name
            )));
        }
    }
    for field in &spec.input {
        match input.get(&field.name) {
            Some(value) => crate::typed::validate(ir, &field.type_ref, value).map_err(|error| {
                ExecuteError::InvalidInput(format!("`{}`: {error}", field.name))
            })?,
            None if field.type_ref.is_optional() => {}
            None => {
                return Err(ExecuteError::InvalidInput(format!(
                    "the input `{}` is required",
                    field.name
                )));
            }
        }
    }
    Ok(())
}
