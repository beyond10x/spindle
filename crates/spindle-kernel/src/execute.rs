//! One command, from input to committed change.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EventHandle, PrecedencePlan, ResolvedCommand, ResolvedCondition, ResolvedEffect,
    ResolvedEntity, ResolvedField, ResolvedInstance, ResolvedOutcome, ResolvedPayloadField,
    ResolvedPayloadValue, ResolvedRelatedTest, ResolvedRelatedVia, ResolvedRowSelection,
    ResolvedRowSetTest, ResolvedTypeRef, RowMember,
};
use ess_domain::QualifiedName;
use ess_domain::command::precedence::Phase;
use ess_primitives::predicate::{Predicate, Truth};
use ess_primitives::time::Rfc3339Instant;
use serde_json::{Map, Value};
use spindle_capability::mint::Unsourced;

use crate::decider::{Decider, External};
use crate::facts::{Facts, Selector, Walked};
use crate::mint::{Generator, IDENTITY_MINT, Identity, Mint, UuidV4};
use crate::store::{Change, Row, Scan, Scanned, Store, Stored, Write};
use crate::values::Sources;
use crate::{DeclaredError, Event, ExecuteError, Executed};

mod affects;
mod delete;
mod set;

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

/// [`at_rest`] for a resolved entity. An entity that declares no invariant reads nothing of the
/// row, so no facts are built for it. The invariants of a newtype a field is declared by are
/// not read here: a value is checked against its type where it is taken in
/// ([`crate::typed::validate`]).
fn rests(
    ir: &EssIr,
    entity: &ResolvedEntity,
    identity: &Value,
    row: &Row,
) -> Result<(), ExecuteError> {
    if entity.invariants.is_empty() {
        return Ok(());
    }
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
/// change in one [`Store::commit`]. A refusal commits nothing; the events of an outcome that
/// writes no row (no subject, `preserves:`, a set outcome that selected no row) are a write of
/// their own ([`Store::commit_events`]), conditioned on the run's scans as a write group is; a
/// set outcome (`instances:`), and an outcome with secondary effects (`affects:`), writes every
/// row it changes, with its events, in one [`Store::commit_all`]. An
/// identity the runtime mints is a random UUIDv4 ([`UuidV4`]).
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
/// `mint` too; a generated `Timestamp` is the current civil time, the browser's on
/// `wasm32-unknown-unknown` ([`execute_at`] takes a clock instead).
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
    execute_at(ir, store, decider, mint, &civil_now, command, input)
}

/// The current civil time: the operating system's, or on `wasm32-unknown-unknown`, where the
/// standard library has no clock, the browser's `Date.now()` (through `web-time`).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn civil_now() -> SystemTime {
    SystemTime::now()
}

/// The current civil time: the operating system's, or on `wasm32-unknown-unknown`, where the
/// standard library has no clock, the browser's `Date.now()` (through `web-time`).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) fn civil_now() -> SystemTime {
    let since = web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    SystemTime::UNIX_EPOCH + since
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
/// A command sent as an actor whose `may:` does not name it, or as an actor the model does not
/// declare, is [`ExecuteError::NotGranted`] before anything else is checked or read, as ESS's
/// interpreter refuses it; a command sent as no actor is not checked. The caller is bound against the attributes of the actor it is sent as, as ESS binds it, before
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
    execute_refusing(ir, store, decider, mint, now, invocation, &BTreeMap::new())
}

/// [`execute_as`], refusing besides each outcome the kernel does not take those `refused` names,
/// by outcome name, each with what the caller does not serve of it: an ingress that cannot answer
/// an outcome as its binding declares.
///
/// A construct selection reads and the kernel does not run refuses the command before anything is
/// read ([`crate::selects`]), and so does a command none of whose outcomes runs, naming its first
/// refused outcome. Otherwise an outcome the kernel does not take ([`crate::takes`]) or `refused`
/// names refuses only the request that selects it: [`ExecuteError::Unsupported`] naming the
/// outcome and what it lacks, once selection took it and before anything is minted or written.
/// Every other outcome runs.
///
/// # Errors
/// As [`execute`].
pub fn execute_refusing(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    now: &dyn Fn() -> SystemTime,
    invocation: &Invocation<'_>,
    refused: &BTreeMap<String, String>,
) -> Result<Executed, ExecuteError> {
    reaching(ir, store, decider, mint, now, invocation, refused, None)
}

/// [`execute_refusing`], on a `store` that reaches the stores of the components `reached` names:
/// each read of an entity one of them owns is answered from its owner's store, as a host running
/// them in one process routes it, so a value, a `when_related:` guard or a row set over such an
/// entity runs where [`execute_refusing`] refuses it ([`crate::takes_reaching`]). Every write goes
/// to the running component's store; an `affects:` entry over another component's entity stays
/// refused.
///
/// # Errors
/// As [`execute`].
#[allow(clippy::too_many_arguments)]
pub fn execute_reaching(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    now: &dyn Fn() -> SystemTime,
    invocation: &Invocation<'_>,
    refused: &BTreeMap<String, String>,
    reached: &BTreeSet<String>,
) -> Result<Executed, ExecuteError> {
    reaching(
        ir,
        store,
        decider,
        mint,
        now,
        invocation,
        refused,
        Some(reached),
    )
}

/// [`execute_refusing`], reaching the stores `reached` names, or the running component's alone.
#[allow(clippy::too_many_arguments)]
fn reaching(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    mint: &mut dyn Mint,
    now: &dyn Fn() -> SystemTime,
    invocation: &Invocation<'_>,
    refused: &BTreeMap<String, String>,
    reached: Option<&BTreeSet<String>>,
) -> Result<Executed, ExecuteError> {
    let Invocation {
        command,
        input,
        actor,
        caller,
    } = *invocation;
    crate::grant::check(ir, command, actor)?;
    let spec = ir
        .commands()
        .iter()
        .find_map(|(name, spec)| (name.to_string() == command).then_some(spec))
        .ok_or_else(|| ExecuteError::UnknownCommand(command.to_owned()))?;
    crate::supported::selects(spec)?;
    if let Some(first) = none_runs(ir, spec, refused, reached) {
        return Err(first);
    }
    let attributes = crate::caller::bind(ir, actor, caller)?;
    check_input(ir, spec, input)?;
    let address = match addressed(ir, spec)? {
        Some((entity, field)) => {
            let identity = input
                .get(&field.name)
                .filter(|identity| !identity.is_null())
                .ok_or_else(|| {
                    ExecuteError::InvalidInput(format!("`{}` names no instance", field.name))
                })?
                .clone();
            Some((entity, field, identity))
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
        held: None,
        related: crate::values::Related::new(),
        selected: crate::values::Selections::new(),
        generator: Generator::new(mint, now, command),
        instant: Cell::new(None),
        minted: None,
        answered: BTreeMap::new(),
        scanned: Vec::new(),
        answer: None,
        refused,
        reached,
        open: None,
    };
    if let Some(executed) = run.before_row()? {
        return Ok(executed);
    }
    run.held = match address {
        Some((entity, field, identity)) => {
            let stored = run.store.load(&entity.name.to_string(), &identity)?;
            Some(Addressed {
                entity,
                field,
                identity,
                stored,
            })
        }
        None => None,
    };
    run.related = crate::values::related(ir, spec, input, &mut *run.store)?;
    match run.select()? {
        Selected::Outcome(outcome) => run.take(outcome),
        Selected::Answered(executed) => Ok(executed),
    }
}

/// Refuses `outcome` of `spec` where `refused` names it, with what the caller does not serve of
/// it, or where the kernel does not take it on a store reaching `reached`
/// ([`crate::takes_reaching`]), or the running component's alone ([`crate::takes`]).
fn outcome_runs(
    ir: &EssIr,
    spec: &ResolvedCommand,
    outcome: &ResolvedOutcome,
    refused: &BTreeMap<String, String>,
    reached: Option<&BTreeSet<String>>,
) -> Result<(), ExecuteError> {
    if let Some(what) = refused.get(outcome.name.as_str()) {
        return Err(ExecuteError::Unsupported {
            construct: format!("{what}, in `{}/{}`", spec.name, outcome.name.as_str()),
        });
    }
    crate::supported::taking(ir, spec, outcome, reached)
}

/// The refusal of the first outcome of `spec`, where none of its outcomes runs
/// ([`outcome_runs`]); `None` where one does, or the command declares none.
fn none_runs(
    ir: &EssIr,
    spec: &ResolvedCommand,
    refused: &BTreeMap<String, String>,
    reached: Option<&BTreeSet<String>>,
) -> Option<ExecuteError> {
    let mut first = None;
    for outcome in &spec.outcomes {
        match outcome_runs(ir, spec, outcome, refused, reached) {
            Ok(()) => return None,
            Err(refusal) => {
                first.get_or_insert(refusal);
            }
        }
    }
    first
}

/// The instance the command addresses, as read before deciding.
struct Addressed<'ir> {
    entity: &'ir ResolvedEntity,
    /// The input field naming the instance.
    field: &'ir ResolvedField,
    identity: Value,
    stored: Option<Stored>,
}

/// What a `when_related:` reference names, read as the store held it before selection (ESS
/// `interpret::execute::related::Reference`).
enum Reference {
    /// No identity: an `Optional` reference left absent. No row is read, and no related branch is
    /// selected.
    Absent,
    /// An identity no row of the entity carries: the `exists: false` branch's answer.
    Missing,
    /// The row it names, with its identity.
    Row(Value, Row),
}

/// What the addressed row answers for the branch a look-ahead selects ([`Run::look_ahead`]).
enum Ahead {
    /// The branch acts on a row no store holds: the existence answer.
    Unknown,
    /// The branch moves the row from a state it does not hold: the wrong-state answer.
    WrongState,
}

/// Whether `phase` of `plan` holds an `exists: false` branch over a row the input names: the
/// phase a missing such row is answered in (ESS `related::reads_missing`).
fn reads_missing(plan: &PrecedencePlan<'_>, phase: Phase) -> bool {
    plan.branches(phase).iter().any(|outcome| {
        matches!(
            outcome.condition,
            ResolvedCondition::Related {
                via: ResolvedRelatedVia::Input { .. },
                test: ResolvedRelatedTest::Absent,
                ..
            }
        )
    })
}

/// Whether `outcome` reads a related row or a row set: in [`Phase::PresentRelated`], a branch a
/// look-ahead leaves out (ESS `present_related_refusal`).
fn present_related(outcome: &ResolvedOutcome) -> bool {
    matches!(
        outcome.condition,
        ResolvedCondition::Related { .. } | ResolvedCondition::RelatedSet { .. }
    )
}

/// Whether a branch of `spec` is guarded by a row set (ESS `row_set::uses`).
fn uses_row_sets(spec: &ResolvedCommand) -> bool {
    spec.outcomes
        .iter()
        .any(|outcome| matches!(outcome.condition, ResolvedCondition::RelatedSet { .. }))
}

/// The one outcome `spec` declares as its answer for an identity no row carries, where it declares
/// no `unknown_instance:` (ESS `synthesize::declared_not_found`, beyond10x/ess#291): an
/// `external:` refusal acting on no row and replaying nothing, whose error carries a field of the
/// type of an input naming the row a `moves:`, `updates:` or `deletes:` branch acts on. `None`
/// where none qualifies, or several do. No outcome or error name is read.
fn declared_not_found<'ir>(ir: &EssIr, spec: &'ir ResolvedCommand) -> Option<&'ir ResolvedOutcome> {
    let identities: Vec<_> = spec
        .outcomes
        .iter()
        .filter_map(|outcome| outcome.subject.as_ref())
        .filter(|subject| {
            matches!(
                subject.effect,
                ResolvedEffect::Moves { .. } | ResolvedEffect::Updates | ResolvedEffect::Deletes
            )
        })
        .filter_map(|subject| match &subject.instance {
            ResolvedInstance::Supplied { field } => Some(field.name.as_str()),
            ResolvedInstance::Observed { .. } => None,
        })
        .filter_map(|field| spec.input.iter().find(|input| input.name == field))
        .map(|input| input.type_ref.required())
        .collect();
    let mut declared = spec.outcomes.iter().filter(|outcome| {
        outcome.subject.is_none()
            && outcome.replays.is_none()
            && outcome.refuses
            && matches!(outcome.condition, ResolvedCondition::External { .. })
            && outcome.error.as_ref().is_some_and(|error| {
                ir.error(error)
                    .fields
                    .iter()
                    .any(|field| identities.contains(&field.type_ref.required()))
            })
    });
    match (declared.next(), declared.next()) {
        (Some(outcome), None) => Some(outcome),
        _ => None,
    }
}

/// Whether `predicate` reads the addressed subject, under `subject.` (ESS
/// `row_set::reads_subject`).
fn reads_subject(predicate: &Predicate) -> bool {
    predicate.fact_paths().iter().any(|path| {
        path.segments().len() > 1
            && path.namespace() == ess_domain::command::set_effects::SUBJECT_NAMESPACE
    })
}

/// The stored rows a row-set guard's predicates leave `Unknown`, for the refusal that names them
/// ([`Run::row_set`]): those its filter leaves open, and those of a `forall` test it does not.
#[derive(Default)]
struct Open {
    filter: Unknowns,
    test: Unknowns,
}

/// The rows one predicate leaves `Unknown`: how many, and every `Optional` field it reads that one
/// of them holds nothing in. The refusal names neither the count nor a row: a row-set guard reads
/// rows the request did not name, which its caller may not read.
#[derive(Default)]
struct Unknowns {
    rows: usize,
    absent: Vec<String>,
}

impl Unknowns {
    /// Counts a row of `entity`, which `predicate` leaves `Unknown`.
    fn read(&mut self, predicate: &Predicate, entity: &ResolvedEntity, row: &Row) {
        self.rows += 1;
        for path in predicate.fact_paths() {
            let [field] = path.segments() else {
                continue;
            };
            let optional = entity
                .fields
                .iter()
                .any(|declared| declared.name == *field && declared.type_ref.is_optional());
            if optional
                && row.fields.get(field).is_none_or(Value::is_null)
                && !self.absent.contains(field)
            {
                self.absent.push(field.clone());
            }
        }
    }

    /// Reads the `Optional` input fields `predicate` reads that `input` holds nothing in: at
    /// their own names, as an input guard reads them, or under `prefix.` where one is given, as a
    /// guard over a row reads the input (`input.<field>`).
    fn read_input(
        &mut self,
        predicate: &Predicate,
        prefix: Option<&str>,
        declared: &[ResolvedField],
        input: &Map<String, Value>,
    ) {
        for path in predicate.fact_paths() {
            let field = match (prefix, path.segments()) {
                (None, [field]) => field,
                (Some(prefix), [first, field]) if first.as_str() == prefix => field,
                _ => continue,
            };
            let optional = declared
                .iter()
                .any(|declared| declared.name == *field && declared.type_ref.is_optional());
            let written = path.to_string();
            if optional
                && input.get(field).is_none_or(Value::is_null)
                && !self.absent.contains(&written)
            {
                self.absent.push(written);
            }
        }
    }

    /// What `predicate`, the guard's `place`, leaves open on these rows of `entity`, and the way
    /// out where an absent `Optional` field is why: `defined(<field>)` beside it reads such a row
    /// `false` (ESS `docs/design/cross-record-and-stored-field-guards.md`, "Optional fields").
    fn why(&self, entity: &ResolvedEntity, predicate: &Predicate, place: &str) -> String {
        self.told(
            format!("`{predicate}` is unknown on a `{}` row", entity.name),
            place,
            "such a row",
        )
    }

    /// `why`, followed by the absent fields and the way out, where there are any:
    /// `defined(<field>)` in the guard's `place` rules `what` out.
    fn told(&self, mut why: String, place: &str, what: &str) -> String {
        if !self.absent.is_empty() {
            let quoted = |format: fn(&String) -> String, separator: &str| {
                self.absent
                    .iter()
                    .map(format)
                    .collect::<Vec<_>>()
                    .join(separator)
            };
            why.push_str(&format!(
                " where {} is absent; write {} in the {place} to rule {what} out",
                quoted(|field| format!("`{field}`"), " or "),
                quoted(|field| format!("`defined({field})`"), " and "),
            ));
        }
        why
    }
}

impl Open {
    /// What left the guard open: the filter's unknown rows where there are any, else the test's.
    fn why(&self, entity: &ResolvedEntity, filter: &Predicate, test: Option<&Predicate>) -> String {
        match test {
            Some(test) if self.filter.rows == 0 && self.test.rows > 0 => {
                self.test.why(entity, test, "test")
            }
            _ => self.filter.why(entity, filter, "filter"),
        }
    }
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
    /// The rows the command's related values name, read before anything else ([`crate::values::related`]).
    related: crate::values::Related,
    /// What the selectors of the outcome selection took select, read once it acts
    /// ([`Run::selections`]); empty until then.
    selected: crate::values::Selections,
    generator: Generator<'a>,
    /// The decision's instant, as every guard and row-set selector reads `now`: read from the
    /// clock once per run, on the first read, and `None` inside where the clock spells no instant
    /// ([`Self::now`]).
    instant: Cell<Option<Option<Rfc3339Instant>>>,
    /// The identity this run minted: the creating event that publishes it, and the identity
    /// field's name.
    minted: Option<(&'ir EventHandle, String, Value)>,
    /// What the decider answered for each external branch it was asked about, by outcome name:
    /// asked once per run, whether the look-ahead or selection reads the branch first.
    answered: BTreeMap<String, bool>,
    /// Each entity the run scanned (a row-set guard, an `affects:` entry, a set outcome's
    /// selection), at the version its first scan answered: every write group the selected
    /// outcome commits is conditioned on them ([`crate::Write::scanned`]).
    scanned: Vec<Scanned>,
    /// The response this run answers, prepared before the creation and the events of an outcome
    /// whose payload reads it ([`Self::prepare`]); `None` until then.
    answer: Option<Map<String, Value>>,
    /// The outcomes the caller does not serve, by name, each with why ([`execute_refusing`]).
    refused: &'a BTreeMap<String, String>,
    /// The components whose stores the store reaches ([`execute_reaching`]); `None` for the
    /// running component's alone.
    reached: Option<&'a BTreeSet<String>>,
    /// What left the last row-set guard read `Unknown` open, with its outcome's qualified name
    /// ([`Self::row_set`]): the refusal that guard answers names it ([`Self::why`]).
    open: Option<(String, String)>,
}

impl<'a, 'ir> Run<'a, 'ir> {
    /// The input refusals of a command that declares an external refusal answered before the row
    /// (`before_row: true`, ess/25), read before the addressed row or any related row is: the
    /// plan's [`Phase::InputRefusal`], input-guarded refusals and those external refusals in
    /// declaration order, the first whose guard holds and whose cause the decider says holds
    /// answered with no row, as ESS's interpreter answers it (`interpret::execute`,
    /// `refused_by_input`). `None` where none holds, the command declares no such refusal, or the
    /// plan reads a related row first; selection then reads the rows and goes on
    /// ([`Self::select`]), each cause already asked keeping its answer ([`Self::ask`]).
    fn before_row(&mut self) -> Result<Option<Executed>, ExecuteError> {
        if !self.spec.outcomes.iter().any(|outcome| outcome.before_row) {
            return Ok(None);
        }
        let plan = PrecedencePlan::new(self.spec, self.ir.format());
        if !plan.branches(Phase::RelatedRow).is_empty() {
            return Ok(None);
        }
        for outcome in plan.branches(Phase::InputRefusal) {
            if self.holds(outcome)? {
                return self.refusal(outcome).map(Some);
            }
        }
        Ok(None)
    }

    /// Reads the outcomes in the order of the command's precedence plan; the first whose
    /// condition holds is selected.
    ///
    /// A `when_related:` row is read where ESS's interpreter reads it (`interpret::execute`,
    /// `responding_phases`): a missing row the input names is answered by its `exists: false`
    /// branch at the step the plan places that branch; a stored reference is read with the
    /// addressed row's existence, once the held state answered for the branch the request selects
    /// with no related row read; and where present-related refusals are read after the held state,
    /// the held state answers first for the branch the request selects without them.
    fn select(&mut self) -> Result<Selected<'ir>, ExecuteError> {
        let plan = PrecedencePlan::new(self.spec, self.ir.format());
        let stored = self.spec.outcomes.iter().any(|outcome| {
            matches!(
                &outcome.condition,
                ResolvedCondition::Related {
                    via: ResolvedRelatedVia::Subject { .. },
                    ..
                }
            )
        });
        let missing_late = reads_missing(&plan, Phase::PresentRelated);
        let mut looked_ahead = false;
        for planned in plan.phases() {
            match planned.phase {
                Phase::RelatedRow => {
                    if let Some(answered) = self.existing(&plan, planned.phase)? {
                        return Ok(Selected::Answered(answered));
                    }
                    if !stored
                        && reads_missing(&plan, planned.phase)
                        && let Some(outcome) = self.missing_related(&plan, planned.phase)?
                    {
                        return Ok(Selected::Outcome(outcome));
                    }
                }
                Phase::Existence => {
                    if let Some(answered) = self.existing(&plan, planned.phase)? {
                        return Ok(Selected::Answered(answered));
                    }
                    if let Some(addressed) = &self.held
                        && addressed.stored.is_none()
                        && (stored || !self.creation_takes_absent(addressed))
                    {
                        if let Some(outcome) = planned
                            .branches
                            .iter()
                            .find(|outcome| outcome.condition == ResolvedCondition::UnknownInstance)
                        {
                            return Ok(Selected::Outcome(outcome));
                        }
                        // An instance nobody holds: the command's one declared not-found refusal,
                        // else the wrong-state answer, since it rests in no state any move of this
                        // command starts from (ESS `unknown_instance`).
                        return self.not_found().map(Selected::Answered);
                    }
                    // On a command guarded by a row set the addressed row's held state answers
                    // before any row set is read, where no branch acting on the row may act from
                    // it (ESS `addressed_row`).
                    if uses_row_sets(self.spec)
                        && let Some(row) = self.before()
                        && !self.may_act(row)
                    {
                        return self.wrong_state().map(Selected::Answered);
                    }
                    if stored {
                        if let Some(selected) = self.stored_reference(&plan)? {
                            return Ok(selected);
                        }
                    } else if !missing_late {
                        self.no_missing_row()?;
                    }
                }
                Phase::InputRefusal => {
                    for outcome in &planned.branches {
                        if self.holds(outcome)? {
                            return Ok(Selected::Outcome(outcome));
                        }
                    }
                }
                Phase::HeldState | Phase::PresentRelated | Phase::Accepting | Phase::Default => {
                    if !looked_ahead {
                        looked_ahead = true;
                        let missing = missing_late && !stored && self.missing_reference()?;
                        if !stored
                            && plan
                                .branches(Phase::PresentRelated)
                                .iter()
                                .any(|outcome| present_related(outcome))
                            && let Some(answered) = self.look_ahead(&plan, true, !missing)?
                        {
                            return Ok(Selected::Answered(answered));
                        }
                    }
                    if missing_late && !stored && planned.phase == Phase::PresentRelated {
                        if let Some(outcome) = self.missing_related(&plan, planned.phase)? {
                            return Ok(Selected::Outcome(outcome));
                        }
                        self.no_missing_row()?;
                    }
                    for outcome in &planned.branches {
                        if self.holds(outcome)? {
                            return Ok(Selected::Outcome(outcome));
                        }
                    }
                }
                Phase::InputAbsent => {
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
        match self.truth(outcome, false)? {
            Truth::True => {}
            Truth::False => return Ok(false),
            Truth::Unknown => return Err(self.undecidable(outcome)),
        }
        match &outcome.condition {
            ResolvedCondition::External { cause }
            | ResolvedCondition::ExternalWhen { cause, .. }
            | ResolvedCondition::ExternalSubject { cause, .. } => self.ask(outcome, cause),
            _ => Ok(true),
        }
    }

    /// What `outcome`'s condition evaluates to before any external cause is asked. `ahead` reads
    /// it for a look-ahead ([`Self::look_ahead`]), where a related row not read yet holds nothing.
    fn truth(&mut self, outcome: &ResolvedOutcome, ahead: bool) -> Result<Truth, ExecuteError> {
        Ok(match &outcome.condition {
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
            // `when_state_changes:` holds where the state held immediately before selection is one
            // of `states`: the branch's own move's `from` set, partitioned by whether each state is
            // the one it arrives at, derived by the compiler (ess-compiler `src/resolve.rs:6105-6121`).
            // ESS's interpreter reads it as it reads a held-state guard, the held state then the
            // input guard, conjunctively (ess-conformance `src/interpret/execute/subject.rs:66-75`,
            // `:125`); the plan places it among the held-state branches in declaration order
            // (ess-domain `src/command/precedence.rs:526-532`), and a subject no row holds is
            // answered before it is read (ess-conformance `src/interpret/execute.rs:994-997`).
            ResolvedCondition::StateChange {
                states, predicate, ..
            } => {
                let row = self.held_row(outcome)?;
                let subject = truth(states.iter().any(|held| held.as_str() == row.state));
                both(subject, || self.guard(predicate.as_ref()))
            }
            // A marker, never selected by reading: the wrong-state answer comes from the branch that
            // is selected, when its move does not start from the state held (ESS `act`).
            ResolvedCondition::WrongState => Truth::False,
            // A marker too: on a row-set upsert the plan reads it after the row sets, and the
            // branch selected after them answers a row nobody holds with it
            // ([`Self::unknown_instance`]).
            ResolvedCondition::UnknownInstance => Truth::False,
            ResolvedCondition::Related {
                via,
                entity,
                test,
                input,
                ..
            } => {
                let ir = self.ir;
                let entity = ir.entity(entity);
                match (self.reference(via, entity)?, test) {
                    (Reference::Absent, _) | (Reference::Row(..), ResolvedRelatedTest::Absent) => {
                        Truth::False
                    }
                    (Reference::Missing, ResolvedRelatedTest::Absent) => Truth::True,
                    (Reference::Missing, ResolvedRelatedTest::Holds { .. }) if ahead => {
                        Truth::False
                    }
                    (Reference::Missing, ResolvedRelatedTest::Holds { .. }) => Truth::Unknown,
                    (Reference::Row(identity, row), ResolvedRelatedTest::Holds { predicate }) => {
                        let related =
                            predicate.evaluate(&self.related_facts(entity, &identity, &row));
                        both(related, || self.guard(input.as_ref()))
                    }
                }
            }
            ResolvedCondition::RelatedSet {
                selection,
                test,
                input,
            } => {
                let tested = self.row_set(outcome, selection, test)?;
                both(tested, || self.guard(input.as_ref()))
            }
            other => {
                return Err(ExecuteError::Unsupported {
                    construct: format!(
                        "the condition {other:?} at this step, in `{}/{}`",
                        self.command,
                        outcome.name.as_str()
                    ),
                });
            }
        })
    }

    /// An input guard over the command's input, read at the input fields' own names as `when:`
    /// reads them, the caller under `caller.` and the decision's instant as `now`; no guard holds.
    fn guard(&self, predicate: Option<&Predicate>) -> Truth {
        predicate.map_or(Truth::True, |predicate| {
            predicate.evaluate(
                &Facts::values(self.ir, &self.spec.input, self.input)
                    .with_caller(self.attributes, self.caller)
                    .at(self.now()),
            )
        })
    }

    /// The decision's one instant, which every guard and row-set selector of this run reads as
    /// `now` (ESS `Context::now`): read from the clock on the first read and kept for the run.
    fn now(&self) -> Option<Rfc3339Instant> {
        if let Some(now) = self.instant.get() {
            return now;
        }
        let now = self.generator.instant();
        self.instant.set(Some(now));
        now
    }

    /// The row `via` names among `entity`'s, as the store holds it before selection: an input
    /// field, or a stored field of the addressed instance as it was before the branch (ESS
    /// `interpret::execute::related`). A reference left absent or `null` names no row.
    ///
    /// A list of references (ess/28) is read for a missing row alone, as ESS reads it
    /// (`related::names_missing`, `related::held`): `Missing` where any element names no row, in
    /// list order, else `Absent`, holding no row; an empty list names none.
    fn reference(
        &mut self,
        via: &ResolvedRelatedVia,
        entity: &ResolvedEntity,
    ) -> Result<Reference, ExecuteError> {
        let identity = match via {
            ResolvedRelatedVia::Input { field, .. } => self.input.get(field),
            ResolvedRelatedVia::Subject { field, .. } => {
                self.before().and_then(|row| row.fields.get(field))
            }
        };
        let Some(identity) = identity.filter(|identity| !identity.is_null()).cloned() else {
            return Ok(Reference::Absent);
        };
        if matches!(via.type_ref(), ResolvedTypeRef::List { .. }) {
            let name = entity.name.to_string();
            for element in identity.as_array().into_iter().flatten() {
                if self.store.load(&name, element)?.is_none() {
                    return Ok(Reference::Missing);
                }
            }
            return Ok(Reference::Absent);
        }
        Ok(
            match self.store.load(&entity.name.to_string(), &identity)? {
                Some(stored) => Reference::Row(identity, stored.row),
                None => Reference::Missing,
            },
        )
    }

    /// Whether `test` holds of the rows of `selection`'s entity its selector selects, read from the
    /// store as it was before selection (ESS `interpret::execute::row_set::Reader::decide`).
    ///
    /// Every stored row is a candidate. The selector reads the row's fields, identity and held
    /// state, the input under `input.`, the addressed subject under `subject.` and the decision's
    /// instant as `now` ([`crate::facts::Selector`]). A rows quantifier in the selector or the
    /// test (`exists: {in: {entity: …}, …}`, ess/27) walks every stored row of its entity, read
    /// from the same store once per decision (ESS `Reader::members`); each entity read so is
    /// scanned, so the commit is conditioned on its version too. A row the selector leaves
    /// `Unknown` stays a possible member, and ESS decides the test over the members
    /// ([`ResolvedRowSetTest::decide`]): a test those possible members leave open is `Unknown`,
    /// and what left it open is kept for the refusal ([`Open`]). A selector reading `subject.` on
    /// a command that addresses no held row decides nothing.
    fn row_set(
        &mut self,
        outcome: &ResolvedOutcome,
        selection: &ResolvedRowSelection,
        test: &ResolvedRowSetTest,
    ) -> Result<Truth, ExecuteError> {
        let ir = self.ir;
        let entity = ir.entity(&selection.entity);
        let subject = match &self.held {
            Some(Addressed {
                entity,
                identity,
                stored: Some(stored),
                ..
            }) => Some(Facts::row(ir, entity, identity, &stored.row, None)),
            _ => None,
        };
        if subject.is_none()
            && (reads_subject(&selection.filter) || test.predicate().is_some_and(reads_subject))
        {
            return Err(ExecuteError::Undecidable {
                outcome: format!("{}/{}", self.command, outcome.name.as_str()),
                why: None,
            });
        }
        let input = Facts::values(ir, &self.spec.input, self.input);
        let now = self.now();
        let mut members = Vec::new();
        let mut open = Open::default();
        let names: std::collections::BTreeSet<&str> = std::iter::once(&selection.filter)
            .chain(test.predicate())
            .flat_map(ess_domain::command::related_rows::walked)
            .collect();
        let mut walked = Walked::new();
        for name in names {
            let Some(other) = name
                .parse()
                .ok()
                .and_then(|parsed| ir.entities().get(&parsed))
            else {
                continue;
            };
            let rows = self
                .scan(&other.name.to_string())?
                .into_iter()
                .map(|(identity, stored)| Facts::row(ir, other, &identity, &stored.row, None))
                .collect();
            walked.insert(name.to_owned(), rows);
        }
        let walked = (!walked.is_empty()).then_some(&walked);
        let scan = self.scan(&entity.name.to_string())?;
        for (identity, stored) in scan {
            let row = Facts::row(ir, entity, &identity, &stored.row, None);
            let facts = Selector {
                row: &row,
                input: &input,
                subject: subject.as_ref(),
                now,
                walked,
            };
            let selected = selection.filter.evaluate(&facts);
            let satisfies = match (test.predicate(), selected) {
                (Some(_), Truth::False) | (None, _) => Truth::True,
                (Some(predicate), _) => predicate.evaluate(&facts),
            };
            if selected == Truth::Unknown {
                open.filter.read(&selection.filter, entity, &stored.row);
            } else if let (Some(predicate), Truth::Unknown) = (test.predicate(), satisfies) {
                open.test.read(predicate, entity, &stored.row);
            }
            members.push(RowMember {
                selected,
                satisfies,
            });
        }
        let decided = test.decide(&members);
        if decided == Truth::Unknown {
            self.open = Some((
                self.qualified(outcome),
                open.why(entity, &selection.filter, test.predicate()),
            ));
        }
        Ok(decided)
    }

    /// What each `{related: {entity, where, field}}` selector of `outcome` selects (ess/22), read
    /// from the store as it was before the outcome wrote anything, as ESS's interpreter reads it
    /// (`interpret::execute::row_set::Reader::one`): every stored row of the selector's entity is
    /// a candidate, read as a row-set guard reads it ([`Self::row_set`]), and ESS decides how many
    /// are selected ([`ess_compiler::ir::Selected::of`]). Rows the outcome itself writes are no
    /// candidates. Read once selection took `outcome` and before it mints or writes anything
    /// ([`Self::acts`]), so a value read after the outcome's own writes still reads this snapshot;
    /// what selects no row or several is answered where a value reads it
    /// ([`crate::values::Sources`]). A branch that reads no selector, a refusal among them, scans
    /// nothing for one, and conditions its commit on no such scan.
    fn selections(
        &mut self,
        outcome: &ResolvedOutcome,
    ) -> Result<crate::values::Selections, ExecuteError> {
        use crate::values::Selection;
        use ess_compiler::ir::Selected as Chosen;
        let ir = self.ir;
        let mut selected = crate::values::Selections::new();
        for selection in crate::values::selections(outcome) {
            let key = crate::values::selection_key(ir, selection);
            if selected.contains_key(&key) {
                continue;
            }
            let entity = ir.entity(&selection.entity);
            let subject = match &self.held {
                Some(Addressed {
                    entity,
                    identity,
                    stored: Some(stored),
                    ..
                }) => Some(Facts::row(ir, entity, identity, &stored.row, None)),
                _ => None,
            };
            if subject.is_none() && reads_subject(&selection.filter) {
                selected.insert(key, Selection::Unknown);
                continue;
            }
            let input = Facts::values(ir, &self.spec.input, self.input);
            let now = self.now();
            let name = entity.name.to_string();
            let scan = self.store.scan(&name)?;
            if !self.scanned.iter().any(|scanned| scanned.entity == name) {
                self.scanned.push(Scanned {
                    entity: name,
                    version: scan.version,
                });
            }
            let mut members = Vec::new();
            let mut rows = Vec::new();
            for (identity, stored) in scan {
                let row = Facts::row(ir, entity, &identity, &stored.row, None);
                let facts = Selector {
                    row: &row,
                    input: &input,
                    subject: subject.as_ref(),
                    now,
                    walked: None,
                };
                members.push(RowMember {
                    selected: selection.filter.evaluate(&facts),
                    satisfies: Truth::True,
                });
                rows.push((identity, stored.row));
            }
            let chosen = match Chosen::of(&members) {
                Chosen::One(index) => {
                    let (identity, row) = rows.swap_remove(index);
                    Selection::One(identity, row)
                }
                Chosen::None => Selection::None,
                Chosen::Several => Selection::Several,
                Chosen::Unknown => Selection::Unknown,
            };
            selected.insert(key, chosen);
        }
        Ok(selected)
    }

    /// Every stored row of `entity`, read from the store as it was before selection, its version
    /// kept where this run had not scanned the entity yet, so the commit is conditioned on it
    /// (`Run::scanned`).
    fn scan(&mut self, entity: &str) -> Result<Scan, ExecuteError> {
        let scan = self.store.scan(entity)?;
        if !self.scanned.iter().any(|scanned| scanned.entity == entity) {
            self.scanned.push(Scanned {
                entity: entity.to_owned(),
                version: scan.version,
            });
        }
        Ok(scan)
    }

    /// `outcome`'s name qualified by the command's, as a refusal names it.
    fn qualified(&self, outcome: &ResolvedOutcome) -> String {
        format!("{}/{}", self.command, outcome.name.as_str())
    }

    /// The refusal of `outcome`, whose guard read `Unknown`: it names what left a row-set guard
    /// open where the last one read was this outcome's ([`Self::row_set`]), and otherwise each part
    /// of the guard an absent `Optional` field left open ([`Self::absent`]).
    fn undecidable(&mut self, outcome: &ResolvedOutcome) -> ExecuteError {
        let qualified = self.qualified(outcome);
        let why = match self.open.take_if(|(open, _)| *open == qualified) {
            Some((_, why)) => Some(why),
            None => self.absent(outcome),
        };
        ExecuteError::Undecidable {
            outcome: qualified,
            why,
        }
    }

    /// Each part of `outcome`'s guard that reads `Unknown` because an `Optional` field it reads
    /// holds nothing, with that field and the way out, `defined(<field>)` ([`Unknowns::told`]):
    /// the predicate over the held instance or over the related row, and the input guard. ESS
    /// reads a comparison, an ordering, `in`, `none_of` or a bare boolean over an absent field as
    /// `Unknown`, and selects no outcome (ESS
    /// `docs/design/cross-record-and-stored-field-guards.md`, "Optional fields"). `None` where no
    /// absent field is why, such as a guard over `now` with no instant.
    fn absent(&mut self, outcome: &ResolvedOutcome) -> Option<String> {
        let ir = self.ir;
        let mut parts = Vec::new();
        let input = match &outcome.condition {
            ResolvedCondition::When { predicate }
            | ResolvedCondition::ExternalWhen { predicate, .. } => Some(predicate),
            ResolvedCondition::SubjectPredicate { predicate, input }
            | ResolvedCondition::ExternalSubject {
                predicate, input, ..
            } => {
                if let (Some(held), Some(row)) = (&self.held, self.before())
                    && let Ok(facts) = self.subject_facts(outcome)
                    && predicate.evaluate(&facts) == Truth::Unknown
                {
                    parts.extend(self.row_part(predicate, held.entity, row));
                }
                input.as_ref()
            }
            // ESS admits `when_subject: {field, equals}` over a required enum field alone, which a
            // held row always holds.
            ResolvedCondition::SubjectField { predicate, .. }
            | ResolvedCondition::SubjectState { predicate, .. }
            | ResolvedCondition::StateChange { predicate, .. } => predicate.as_ref(),
            ResolvedCondition::RelatedSet { input, .. } => input.as_ref(),
            ResolvedCondition::Related {
                via,
                entity,
                test: ResolvedRelatedTest::Holds { predicate },
                input,
                ..
            } => {
                let entity = ir.entity(entity);
                if let Ok(Reference::Row(identity, row)) = self.reference(via, entity)
                    && predicate.evaluate(&self.related_facts(entity, &identity, &row))
                        == Truth::Unknown
                {
                    parts.extend(self.row_part(predicate, entity, &row));
                }
                input.as_ref()
            }
            _ => None,
        };
        if let Some(guard) = input
            && self.guard(Some(guard)) == Truth::Unknown
        {
            let mut unknowns = Unknowns::default();
            unknowns.read_input(guard, None, &self.spec.input, self.input);
            if !unknowns.absent.is_empty() {
                parts.push(unknowns.told(
                    format!("`{guard}` is unknown on the input"),
                    "guard",
                    "such a request",
                ));
            }
        }
        (!parts.is_empty()).then(|| parts.join("; and "))
    }

    /// What `predicate`, a guard over `row` of `entity`, leaves open where an `Optional` field of
    /// the row, or of the input it reads under `input.`, holds nothing; `None` where none is why.
    fn row_part(
        &self,
        predicate: &Predicate,
        entity: &ResolvedEntity,
        row: &Row,
    ) -> Option<String> {
        let mut unknowns = Unknowns::default();
        unknowns.read(predicate, entity, row);
        unknowns.read_input(
            predicate,
            Some(crate::facts::INPUT),
            &self.spec.input,
            self.input,
        );
        (!unknowns.absent.is_empty()).then(|| unknowns.why(entity, predicate, "guard"))
    }

    /// The `existing_instance:` answer where the plan places that branch in `phase`, step 1 on a
    /// command reading a related row or a row set before the one rule and the existence step
    /// otherwise: a row already carries the identity a creation of the command takes from the
    /// input. `None` where `phase` holds no such branch or no row carries the identity, and
    /// selection goes on (ESS `interpret::execute::existence::existing`). Every creation shares
    /// one address, since admission refuses the branch beside creations of different addresses
    /// ([`crate::selects`]). The branch is answered as ESS answers it: its declared error, read
    /// with no row, and nothing written or emitted.
    fn existing(
        &mut self,
        plan: &PrecedencePlan<'ir>,
        phase: Phase,
    ) -> Result<Option<Executed>, ExecuteError> {
        let Some(refused) = plan
            .branches(phase)
            .iter()
            .copied()
            .find(|outcome| outcome.condition == ResolvedCondition::ExistingInstance)
        else {
            return Ok(None);
        };
        let ir = self.ir;
        for creation in &self.spec.outcomes {
            let Some((entity, Some(field))) = crate::supported::creation_address(creation) else {
                continue;
            };
            // The source reads a field or a nested member (`wrap.id`), as ESS `read_input` does.
            let Some(identity) = crate::values::read_input(self.input, field) else {
                continue;
            };
            let entity = ir.entity(entity).name.to_string();
            if self.store.load(&entity, &identity)?.is_some() {
                self.runs(refused)?;
                return Ok(Some(Executed {
                    outcome: refused.name.as_str().to_owned(),
                    events: Vec::new(),
                    error: self.declared_error(refused, None)?,
                    response: self.response(refused)?,
                }));
            }
        }
        Ok(None)
    }

    /// Whether the absent `addressed` row is a creation's to take rather than an unknown instance:
    /// no branch is guarded by the row's held state, which answers a missing row first (ESS
    /// `held_rows`), and a creation branch of the same entity takes its identity from the input
    /// field that names the row (ESS `addressed_row`, `addressed_unknown`,
    /// `creation_takes_absent`). Selection then goes on, and the `unknown_instance:` answer applies
    /// only where the branch it selects acts on the row ([`Self::unknown_instance`]); on a command
    /// guarded by a row set the row sets answer first, then the creation: an upsert
    /// (beyond10x/ess#462). A stored reference answers a missing row before either (ESS
    /// `stored_reference`), so its caller asks only without one.
    fn creation_takes_absent(&self, addressed: &Addressed<'ir>) -> bool {
        let ir = self.ir;
        !self.spec.outcomes.iter().any(|outcome| {
            matches!(
                outcome.condition,
                ResolvedCondition::SubjectState { .. }
                    | ResolvedCondition::StateChange { .. }
                    | ResolvedCondition::SubjectField { .. }
                    | ResolvedCondition::SubjectPredicate { .. }
                    | ResolvedCondition::ExternalSubject { .. }
            )
        }) && self.spec.outcomes.iter().any(|outcome| {
            let Some(subject) = outcome.subject.as_ref().filter(|_| outcome.error.is_none()) else {
                return false;
            };
            let (ResolvedEffect::Creates, ResolvedInstance::Observed { event, field }) =
                (&subject.effect, &subject.instance)
            else {
                return false;
            };
            ir.entity(&subject.entity).name == addressed.entity.name
                && outcome
                    .payload
                    .iter()
                    .filter(|payload| payload.event == *event)
                    .flat_map(|payload| &payload.fields)
                    .find(|source| source.target == field.name)
                    .is_some_and(|source| {
                        matches!(
                            &source.value,
                            ResolvedPayloadValue::InputField { field: taken, .. }
                            | ResolvedPayloadValue::InputOrGenerated { field: taken, .. }
                                if *taken == addressed.field.name
                        )
                    })
        })
    }

    /// Whether some branch of the command that acts on the addressed row may act on it in the
    /// state `row` holds: a move from that state, or any other effect on it. `true` where no
    /// branch acts on it (ESS `addressed_row`).
    fn may_act(&self, row: &Row) -> bool {
        let mut acting = self
            .spec
            .outcomes
            .iter()
            .filter(|outcome| outcome.error.is_none())
            .filter_map(|outcome| outcome.subject.as_ref())
            .filter(|subject| {
                !matches!(subject.effect, ResolvedEffect::Creates)
                    && matches!(subject.instance, ResolvedInstance::Supplied { .. })
            })
            .peekable();
        acting.peek().is_none()
            || acting.any(|subject| match &subject.effect {
                ResolvedEffect::Moves { transition } => transition
                    .from
                    .iter()
                    .any(|state| state.as_str() == row.state),
                _ => true,
            })
    }

    /// The facts a guard over a related row reads: the row, the input under `input.`, the caller
    /// under `caller.` and the decision's instant as `now`, as [`Self::subject_facts`] reads the
    /// addressed one.
    fn related_facts(&self, entity: &'ir ResolvedEntity, identity: &Value, row: &Row) -> Facts<'_> {
        let shadowed = entity
            .fields
            .iter()
            .any(|field| field.name == crate::facts::INPUT);
        let input = (!shadowed).then_some((self.spec.input.as_slice(), self.input));
        Facts::row(self.ir, entity, identity, row, input)
            .with_caller(self.attributes, self.caller)
            .at(self.now())
    }

    /// The `exists: false` branch answering the first missing row the input names, the rows read
    /// in the order of their `exists: false` branches in `phase`, then any other row in
    /// declaration order (ESS `related::reads_in_order`, `related_absent`). `None` where every row
    /// is present or absent, or the first missing one has no `exists: false` branch: that row is
    /// [`Self::no_missing_row`]'s.
    fn missing_related(
        &mut self,
        plan: &PrecedencePlan<'ir>,
        phase: Phase,
    ) -> Result<Option<&'ir ResolvedOutcome>, ExecuteError> {
        let spec = self.spec;
        let ir = self.ir;
        let mut read: Vec<&str> = Vec::new();
        for outcome in plan.branches(phase).iter().copied().chain(&spec.outcomes) {
            let ResolvedCondition::Related { via, entity, .. } = &outcome.condition else {
                continue;
            };
            if read.contains(&via.field()) {
                continue;
            }
            read.push(via.field());
            if !matches!(via, ResolvedRelatedVia::Input { .. }) {
                continue;
            }
            if let Reference::Missing = self.reference(via, ir.entity(entity))? {
                return Ok(spec.outcomes.iter().find(|absent| {
                    matches!(
                        &absent.condition,
                        ResolvedCondition::Related {
                            via: named,
                            test: ResolvedRelatedTest::Absent,
                            ..
                        } if named.field() == via.field()
                    )
                }));
            }
        }
        Ok(None)
    }

    /// Whether a related row the command reads, through the input or a stored reference, is named
    /// and no store holds it: the request is then the model's to answer, by that row's
    /// `exists: false` branch or as undecidable, unless the held state answers first, and a
    /// decider is asked only where a branch it could select has a held-state answer, or a guard
    /// past an external branch is unknown ([`Self::look_ahead`]).
    fn missing_reference(&mut self) -> Result<bool, ExecuteError> {
        let spec = self.spec;
        let ir = self.ir;
        for outcome in &spec.outcomes {
            if let ResolvedCondition::Related { via, entity, .. } = &outcome.condition
                && let Reference::Missing = self.reference(via, ir.entity(entity))?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Refuses a missing row the input names that no `exists: false` branch answered: a predicate
    /// over it is `Unknown`, never a branch taken by default, and ESS's interpreter refuses the
    /// whole selection where it reads it.
    fn no_missing_row(&mut self) -> Result<(), ExecuteError> {
        let spec = self.spec;
        let ir = self.ir;
        for outcome in &spec.outcomes {
            if let ResolvedCondition::Related {
                via: via @ ResolvedRelatedVia::Input { .. },
                entity,
                ..
            } = &outcome.condition
                && let Reference::Missing = self.reference(via, ir.entity(entity))?
            {
                return Err(ExecuteError::Undecidable {
                    outcome: format!("{}/{}", self.command, outcome.name.as_str()),
                    why: None,
                });
            }
        }
        Ok(())
    }

    /// The row a stored reference names, read once the addressed row exists: the held state first
    /// answers for the branch the request selects with no related row read, then a missing row
    /// takes the command's `exists: false` branch (ESS `stored_reference`). `None` where selection
    /// goes on, with the row present or the reference absent.
    fn stored_reference(
        &mut self,
        plan: &PrecedencePlan<'ir>,
    ) -> Result<Option<Selected<'ir>>, ExecuteError> {
        let missing = self.missing_reference()?;
        if let Some(answered) = self.look_ahead(plan, false, !missing)? {
            return Ok(Some(Selected::Answered(answered)));
        }
        let spec = self.spec;
        let ir = self.ir;
        let Some((outcome, via, entity)) = spec.outcomes.iter().find_map(|outcome| match &outcome
            .condition
        {
            ResolvedCondition::Related { via, entity, .. } => Some((outcome, via, entity)),
            _ => None,
        }) else {
            return Ok(None);
        };
        if self.held.is_none() {
            return Err(self.no_subject(outcome));
        }
        let Reference::Missing = self.reference(via, ir.entity(entity))? else {
            return Ok(None);
        };
        let absent = spec.outcomes.iter().find(|absent| {
            matches!(
                absent.condition,
                ResolvedCondition::Related {
                    test: ResolvedRelatedTest::Absent,
                    ..
                }
            )
        });
        match absent {
            Some(absent) => Ok(Some(Selected::Outcome(absent))),
            None => Err(ExecuteError::Undecidable {
                outcome: format!("{}/{}", self.command, outcome.name.as_str()),
                why: None,
            }),
        }
    }

    /// The addressed row's held-state answer for the branch the request selects with the
    /// present-related refusals left out, where that branch moves the row from a state it does
    /// not hold (ESS `subject_refusals_before_present_related`, `selected_subject_refusals`).
    /// `subjects: false` leaves the held-state guards out too, as ESS's look-ahead before a stored
    /// reference reads no held row, so an external branch narrowed by one (`ExternalSubject`)
    /// passes there.
    ///
    /// An external branch is read as ESS's `select` reads it under the request's provider
    /// answer: once every other part of its condition holds, the decider is asked, and a branch it
    /// takes is the selected one, so nothing after it is read and no decider is asked past it.
    /// The answer is kept for selection ([`Self::ask`]). A selected refusal acts on no row, so
    /// selection goes on to the related rows.
    ///
    /// `ask: false` is the look-ahead beside a missing related row ([`Self::missing_reference`]),
    /// which the model answers itself once the held state has. A first walk asks no decider and
    /// reads past each external branch. Where no branch it could select, the external ones passed
    /// included, has a held-state answer, nothing is asked and the missing row answers. The
    /// decider is asked, by the walk that asks, wherever any branch it could select has a
    /// held-state answer, and where a guard read past an external branch is unknown.
    fn look_ahead(
        &mut self,
        plan: &PrecedencePlan<'ir>,
        subjects: bool,
        ask: bool,
    ) -> Result<Option<Executed>, ExecuteError> {
        if !ask {
            let mut passed = Vec::new();
            match self.select_ahead(plan, subjects, Some(&mut passed)) {
                Ok(selected) => {
                    if selected
                        .into_iter()
                        .chain(passed)
                        .all(|outcome| self.ahead_answer(outcome).is_none())
                    {
                        return Ok(None);
                    }
                }
                // A guard read past an external branch is unknown: a decider that takes that
                // branch stops the reading before it, so the walk that asks decides.
                Err(ExecuteError::Undecidable { .. }) if !passed.is_empty() => {}
                Err(error) => return Err(error),
            }
        }
        let Some(outcome) = self.select_ahead(plan, subjects, None)? else {
            return Ok(None);
        };
        match self.ahead_answer(outcome) {
            None => Ok(None),
            Some(Ahead::Unknown) => self.unknown_instance(outcome).map(Some),
            Some(Ahead::WrongState) => self.wrong_state().map(Some),
        }
    }

    /// The branch the look-ahead selects ([`Self::look_ahead`]). With `passed`, no decider is
    /// asked: an external branch whose condition otherwise holds is collected there and reading
    /// goes on past it, as under an open provider.
    fn select_ahead(
        &mut self,
        plan: &PrecedencePlan<'ir>,
        subjects: bool,
        mut passed: Option<&mut Vec<&'ir ResolvedOutcome>>,
    ) -> Result<Option<&'ir ResolvedOutcome>, ExecuteError> {
        // An absent addressed row reaches here only where a creation takes it
        // ([`Self::creation_takes_absent`]). No decider is asked for it here: an external branch
        // stays one possible answer and reading goes on, as under an open provider. A branch
        // selected that acts on the row without creating it is answered for the missing row,
        // except beside row sets, which answer first (ESS `selected_subject_refusal`).
        let absent = self.held.as_ref().is_some_and(|held| held.stored.is_none());
        for planned in plan.phases() {
            match planned.phase {
                Phase::HeldState | Phase::PresentRelated | Phase::Accepting | Phase::Default => {}
                Phase::InputAbsent | Phase::RelatedRow | Phase::InputRefusal | Phase::Existence => {
                    continue;
                }
            }
            for outcome in &planned.branches {
                let skipped = match &outcome.condition {
                    ResolvedCondition::Related { .. } | ResolvedCondition::RelatedSet { .. } => {
                        planned.phase == Phase::PresentRelated
                    }
                    ResolvedCondition::SubjectState { .. }
                    | ResolvedCondition::SubjectField { .. }
                    | ResolvedCondition::SubjectPredicate { .. }
                    | ResolvedCondition::StateChange { .. }
                    | ResolvedCondition::ExternalSubject { .. } => !subjects,
                    _ => false,
                };
                if skipped {
                    continue;
                }
                match self.truth(outcome, true)? {
                    Truth::True => {
                        let taken = match &outcome.condition {
                            ResolvedCondition::External { .. }
                            | ResolvedCondition::ExternalWhen { .. }
                            | ResolvedCondition::ExternalSubject { .. }
                                if absent =>
                            {
                                false
                            }
                            ResolvedCondition::External { cause }
                            | ResolvedCondition::ExternalWhen { cause, .. }
                            | ResolvedCondition::ExternalSubject { cause, .. } => {
                                match passed.as_deref_mut() {
                                    Some(passed) => {
                                        passed.push(*outcome);
                                        false
                                    }
                                    None => self.ask(outcome, cause)?,
                                }
                            }
                            _ => true,
                        };
                        if taken {
                            return Ok(Some(*outcome));
                        }
                    }
                    Truth::False => {}
                    Truth::Unknown => return Err(self.undecidable(outcome)),
                }
            }
        }
        Ok(None)
    }

    /// What the addressed row answers for `outcome`, once the look-ahead selected it
    /// ([`Self::look_ahead`]), or `None` where selection goes on to the related rows.
    fn ahead_answer(&self, outcome: &ResolvedOutcome) -> Option<Ahead> {
        let subject = outcome.subject.as_ref()?;
        let absent = self.held.as_ref().is_some_and(|held| held.stored.is_none());
        // The branch selected acts on the absent row without creating it: the row's existence
        // answers before any present-related refusal, outside row sets, where the creation
        // answers after the row sets instead (ESS `selected_subject_refusal`).
        if absent
            && subject.effect != ResolvedEffect::Creates
            && matches!(subject.instance, ResolvedInstance::Supplied { .. })
            && !uses_row_sets(self.spec)
        {
            return Some(Ahead::Unknown);
        }
        let (ResolvedEffect::Moves { transition }, ResolvedInstance::Supplied { .. }) =
            (&subject.effect, &subject.instance)
        else {
            return None;
        };
        let row = self.before()?;
        (!transition
            .from
            .iter()
            .any(|state| state.as_str() == row.state))
        .then_some(Ahead::WrongState)
    }

    /// The facts a guard over the subject reads: the held instance, the input under `input.` and
    /// the caller under `caller.`, each unless the entity declares a field of that name, which
    /// keeps being read as itself, and the decision's instant as `now`.
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
            .with_caller(self.attributes, self.caller)
            .at(self.now()))
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
    ///
    /// The decider is asked once per branch and run: a branch the look-ahead already asked about
    /// ([`Self::look_ahead`]) keeps that answer when selection reaches it.
    fn ask(&mut self, outcome: &ResolvedOutcome, cause: &str) -> Result<bool, ExecuteError> {
        if let Some(answer) = self.answered.get(outcome.name.as_str()) {
            return Ok(*answer);
        }
        let answer = self.decide(outcome, cause)?;
        self.answered
            .insert(outcome.name.as_str().to_owned(), answer);
        Ok(answer)
    }

    fn decide(&mut self, outcome: &ResolvedOutcome, cause: &str) -> Result<bool, ExecuteError> {
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

    /// The answer for the addressed row no store holds, where `taken`, the branch selection took,
    /// acts on it: the command's `unknown_instance:` branch, taken as selection would take it,
    /// else its one declared not-found refusal ([`declared_not_found`]), else the wrong-state
    /// answer (ESS `take`, `unknown_instance`). Selection leaves such a row to the branch it
    /// selects only where a creation may take it ([`Self::creation_takes_absent`]); `taken` being
    /// the `unknown_instance:` branch itself is the wrong-state answer.
    fn unknown_instance(&mut self, taken: &ResolvedOutcome) -> Result<Executed, ExecuteError> {
        match self
            .spec
            .outcomes
            .iter()
            .find(|outcome| outcome.condition == ResolvedCondition::UnknownInstance)
        {
            Some(outcome) if outcome.name != taken.name => self.take(outcome),
            Some(_) => self.wrong_state(),
            None => self.not_found(),
        }
    }

    /// The answer for the addressed row no store holds on a command that declares no
    /// `unknown_instance:`: its one declared not-found refusal ([`declared_not_found`]), else the
    /// wrong-state answer (ESS `unknown_instance`). The not-found refusal is answered as ESS's
    /// interpreter answers it: an error field it gives no source is left out, not minted, as no
    /// row exists to describe.
    fn not_found(&self) -> Result<Executed, ExecuteError> {
        let Some(outcome) = declared_not_found(self.ir, self.spec) else {
            return self.wrong_state();
        };
        self.runs(outcome)?;
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events: Vec::new(),
            error: self.error_of(outcome, None, false)?,
            response: self.response(outcome)?,
        })
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
                    ..
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
        self.runs(outcome)?;
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events: Vec::new(),
            error: self.declared_error(outcome, self.before())?,
            response: self.response(outcome)?,
        })
    }

    fn before(&self) -> Option<&Row> {
        self.held
            .as_ref()
            .and_then(|held| held.stored.as_ref())
            .map(|stored| &stored.row)
    }

    /// Refuses the selected `outcome` where it does not run ([`outcome_runs`]), before anything is
    /// minted or written.
    fn runs(&self, outcome: &ResolvedOutcome) -> Result<(), ExecuteError> {
        outcome_runs(self.ir, self.spec, outcome, self.refused, self.reached)
    }

    /// Refuses the selected `outcome` where it does not run ([`Self::runs`]), and otherwise reads
    /// what its selectors select ([`Self::selections`]): the outcome is about to act, and nothing
    /// is minted or written yet.
    fn acts(&mut self, outcome: &ResolvedOutcome) -> Result<(), ExecuteError> {
        self.runs(outcome)?;
        self.selected = self.selections(outcome)?;
        Ok(())
    }

    /// Applies `outcome`'s effect and commits it, once it is known to act: an outcome that does not
    /// run ([`Self::runs`]) is refused there, and a move from a state the row does not hold is
    /// the wrong-state answer instead.
    fn take(&mut self, outcome: &'ir ResolvedOutcome) -> Result<Executed, ExecuteError> {
        if let Some(set) = &outcome.instances {
            self.acts(outcome)?;
            return self.take_set(outcome, set);
        }
        let Some(subject) = &outcome.subject else {
            self.acts(outcome)?;
            self.publish_reads(outcome, None)?;
            self.prepare(outcome)?;
            let before = self.before();
            let executed = Executed {
                outcome: outcome.name.as_str().to_owned(),
                events: self.emit(outcome, before)?,
                error: self.declared_error(outcome, before)?,
                response: self.response(outcome)?,
            };
            self.store.commit_events(&executed.events, &self.scanned)?;
            return Ok(executed);
        };
        let entity = self.ir.entity(&subject.entity);
        if let (ResolvedEffect::Deletes, ResolvedInstance::Supplied { .. }) =
            (&subject.effect, &subject.instance)
        {
            self.acts(outcome)?;
            return self.delete(outcome, entity);
        }
        let (change, affected, events) = match (&subject.effect, &subject.instance) {
            (ResolvedEffect::Creates, ResolvedInstance::Observed { event, field }) => {
                self.acts(outcome)?;
                self.publish_reads(outcome, None)?;
                // The creation's identity may itself read the response, so it is prepared first.
                self.prepare(outcome)?;
                let identity = self.creation_identity(outcome, entity, event, field)?;
                let state = subject
                    .into
                    .as_ref()
                    .unwrap_or(&entity.lifecycle.initial)
                    .to_string();
                let mut fields = BTreeMap::new();
                write(
                    &self.sources(outcome, &entity.name, None, None),
                    &outcome.sets,
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
                        scanned: self.scanned.clone(),
                    }),
                    Vec::new(),
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
                    return self.unknown_instance(outcome);
                };
                let (identity, stored) = (identity.clone(), stored.clone());
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
                self.acts(outcome)?;
                self.publish_reads(outcome, Some(&stored.row))?;
                let sources = self.sources(outcome, &entity.name, None, Some(&stored.row));
                write(&sources, &outcome.sets, &mut after.fields)?;
                let affected = self.affect(outcome, entity, &identity, &stored.row)?;
                self.prepare(outcome)?;
                let events = self.emit(outcome, Some(&stored.row))?;
                let change = (!matches!(effect, ResolvedEffect::Preserves)).then(|| Change {
                    entity: entity.name.to_string(),
                    identity,
                    expected: Some(stored.version),
                    row: after,
                    events: events.clone(),
                    scanned: self.scanned.clone(),
                });
                (change, affected, events)
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
        let response = self.response(outcome)?;
        if let Some(change) = &change {
            self.at_rest(entity, change)?;
        }
        if affected.is_empty() {
            match &change {
                Some(change) => self.store.commit(change)?,
                // An outcome that preserves its subject writes no row: its events are a write of
                // their own, conditioned on the run's scans as a group is.
                None => self.store.commit_events(&events, &self.scanned)?,
            }
        } else {
            let writes = self.grouped(change.map(Write::Commit), affected, &events);
            self.store.commit_all(&writes)?;
        }
        Ok(Executed {
            outcome: outcome.name.as_str().to_owned(),
            events,
            error,
            response,
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

    /// The identity of the instance `outcome` creates, published as `field` of `event`: minted
    /// ([`Self::mint_identity`]) where the outcome leaves it to the runtime, else what its source
    /// reads ([`Self::determined_identity`]), a `{generated: true}` fallback among them.
    ///
    /// An `Integer` identity minted in this draw, which many UUIDs mint alike, that an instance of
    /// `entity` already holds is minted again, as ESS's interpreter draws again on a held
    /// identity (`interpret::execute::creation_identity`): at most one draw more than the
    /// instances of `entity` the store holds, read once, on the first held draw.
    ///
    /// # Errors
    /// [`ExecuteError::Undeclared`] for an identity an instance already holds that this draw did
    /// not mint, a minted `Uuid` or text one (minted from a UUID the instance holds), or an
    /// `Integer` one held on every draw, naming the draws; and as the mint, the source or the
    /// store reads.
    fn creation_identity(
        &mut self,
        outcome: &'ir ResolvedOutcome,
        entity: &ResolvedEntity,
        event: &'ir EventHandle,
        field: &ResolvedField,
    ) -> Result<Value, ExecuteError> {
        let redraws = matches!(
            crate::mint::identity(self.ir, &entity.identity.type_ref),
            Some(crate::mint::Minted::Integer)
        );
        let entity_name = entity.name.to_string();
        // The instances of `entity` held, read on the first held draw: ESS draws
        // `0..=occupied`, one candidate beyond the held population.
        let mut occupied = None;
        let mut draws = 0_usize;
        loop {
            draws += 1;
            let mark = self.generator.mark();
            let (identity, minted) =
                if let Some((created, named)) = crate::mint::slot(self.ir, outcome) {
                    let minted = self.mint_identity(outcome, created, named)?;
                    self.minted = Some((event, named.name.clone(), minted.clone()));
                    (minted, true)
                } else {
                    let identity = self.determined_identity(outcome, event, field)?;
                    (identity, self.generator.mark() > mark)
                };
            if self.store.load(&entity_name, &identity)?.is_none() {
                return Ok(identity);
            }
            if !(redraws && minted) {
                return Err(ExecuteError::Undeclared {
                    command: self.command.to_owned(),
                    reason: format!(
                        "a `{entity_name}` already has the identity {identity}, and `creates:` \
                         never replaces an instance"
                    ),
                });
            }
            let held = match occupied {
                Some(held) => held,
                None => {
                    let counts = self.store.counts(std::slice::from_ref(&entity_name))?;
                    let held = counts.get(&entity_name).copied().unwrap_or(0);
                    occupied = Some(held);
                    held
                }
            };
            if draws > held {
                return Err(ExecuteError::Undeclared {
                    command: self.command.to_owned(),
                    reason: format!(
                        "each of {draws} `Integer` identities minted in turn for a new \
                         `{entity_name}`, one more than the {held} it holds, is already held by \
                         one, and `creates:` never replaces an instance"
                    ),
                });
            }
            self.generator.rewind(mark);
        }
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
            related: &self.related,
            selected: &self.selected,
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
            changed: None,
            response: self.answer.as_ref(),
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
            .map(|event| self.event(outcome, event, before, None))
            .collect()
    }

    /// One occurrence of `event`, which `outcome` emits, with the payload fields it determines,
    /// read with the instance as it was `before`; `changed` is how many rows a set outcome changed.
    fn event(
        &self,
        outcome: &ResolvedOutcome,
        event: &EventHandle,
        before: Option<&Row>,
        changed: Option<usize>,
    ) -> Result<Event, ExecuteError> {
        let fields = outcome
            .payload
            .iter()
            .find(|payload| payload.event == *event)
            .map_or(&[][..], |payload| payload.fields.as_slice());
        let mut payload = Sources {
            changed,
            ..self.sources(outcome, event.name(), Some(event), before)
        }
        .object(fields)?;
        // A minted identity is published in the field `instance:` names on the event `instance:`
        // names, where the outcome gives that field no source of its own, and on no other event
        // (ESS `interpret::execute::emit`).
        if let Some((creating, name, minted)) = &self.minted
            && *creating == event
            && !fields.iter().any(|source| source.target == *name)
            && self
                .ir
                .events()
                .get(event.name())
                .is_some_and(|declared| declared.fields.iter().any(|field| field.name == *name))
        {
            payload.insert(name.clone(), minted.clone());
        }
        // A field left absent is carried as `null` where it declares `presence: null_when_absent`,
        // and left out otherwise (ESS `interpret::execute::emit`, beyond10x/ess#765).
        if let Some(declared) = self.ir.events().get(event.name()) {
            for field in &declared.fields {
                if field.naming.presence == Some(ess_domain::types::Presence::NullWhenAbsent) {
                    payload.entry(field.name.clone()).or_insert(Value::Null);
                }
            }
        }
        Ok(Event {
            name: event.name().to_string(),
            payload,
        })
    }

    fn declared_error(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
    ) -> Result<Option<DeclaredError>, ExecuteError> {
        self.error_of(outcome, before, true)
    }

    /// [`Self::declared_error`]; `mint: false` leaves a field that would be minted out
    /// ([`Self::not_found`]).
    fn error_of(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
        mint: bool,
    ) -> Result<Option<DeclaredError>, ExecuteError> {
        // A `wrong_state:` branch that accepts (`refuses: false`) reports no error.
        let Some(error) = outcome.error.as_ref().filter(|_| outcome.refuses) else {
            return Ok(None);
        };
        let sources = Sources {
            error: true,
            ..self.sources(outcome, error.name(), None, before)
        };
        let mut fields = sources.object(&outcome.error_payload)?;
        self.unsourced_error_fields(outcome, before, mint, &mut fields)?;
        Ok(Some(DeclaredError {
            name: error.name().to_string(),
            fields,
            unknown_instance: outcome.condition == ResolvedCondition::WrongState
                && before.is_none(),
        }))
    }

    /// Fills each field of `outcome`'s error that `outcome` gives no source, after the sourced
    /// ones, as [`spindle_capability::mint::unsourced`] says: from the row the refusal is
    /// answered for (`before`), as ESS's served application reads it, else minted from its type,
    /// as ESS's interpreter leaves it to the implementation; an `Optional` one no row fills stays
    /// absent. Without a row (the unknown-instance arm of `wrong_state:`) a field the row would
    /// fill is left out: an instance that does not exist has nothing to describe. With `mint`
    /// false a field that would be minted is left out too.
    fn unsourced_error_fields(
        &self,
        outcome: &ResolvedOutcome,
        before: Option<&Row>,
        mint: bool,
        fields: &mut Map<String, Value>,
    ) -> Result<(), ExecuteError> {
        let Some(error) = outcome.error.as_ref() else {
            return Ok(());
        };
        let owner = error.name().to_string();
        for (field, fill) in spindle_capability::mint::unsourced(self.ir, self.spec, outcome) {
            let value = match (fill, before) {
                (Unsourced::State, Some(row)) => Some(Value::String(row.state.clone())),
                (Unsourced::Stored(stored), Some(row)) => self
                    .held
                    .as_ref()
                    .filter(|held| held.entity.identity.name == stored)
                    .map(|held| held.identity.clone())
                    .or_else(|| row.fields.get(stored).cloned())
                    .filter(|value| !value.is_null()),
                (Unsourced::State | Unsourced::Stored(_), None)
                | (Unsourced::Absent | Unsourced::Unanswered, _) => None,
                (Unsourced::Minted(_), _) if !mint => None,
                (Unsourced::Minted(choice), _) => {
                    let minted = self
                        .generator
                        .choose(
                            &choice,
                            &Identity {
                                capability: IDENTITY_MINT,
                                command: self.command,
                                outcome: outcome.name.as_str(),
                                entity: &owner,
                                field: &field.name,
                            },
                        )
                        .map_err(|why| {
                            ExecuteError::InvalidValue(format!(
                                "the value minted for `{owner}.{}` in `{}/{}` would be {why}",
                                field.name,
                                self.command,
                                outcome.name.as_str()
                            ))
                        })?;
                    crate::typed::validate(self.ir, &field.type_ref, &minted).map_err(|why| {
                        ExecuteError::InvalidValue(format!(
                            "the value minted for `{owner}.{}` in `{}/{}` would hold {why}",
                            field.name,
                            self.command,
                            outcome.name.as_str()
                        ))
                    })?;
                    Some(minted)
                }
                (Unsourced::Unfilled, _) => {
                    return Err(ExecuteError::Unsupported {
                        construct: crate::mint::unfilled(&owner, field),
                    });
                }
            };
            if let Some(value) = value {
                fields.insert(field.name.clone(), value);
            }
        }
        Ok(())
    }

    /// Prepares the response `outcome` answers where an event payload of it reads the response
    /// (`{response: <field>}`, [`crate::supported::reads_response`]), so the payload reads the
    /// value the run answers: where ESS's interpreter prepares it (`interpret::execute::take`),
    /// before a creation takes its identity and its `sets:`, and after an existing row's `sets:`
    /// and secondary effects, before the events. Where no payload reads it, the response is drawn
    /// after everything else the outcome draws ([`Self::response`]), as before.
    fn prepare(&mut self, outcome: &ResolvedOutcome) -> Result<(), ExecuteError> {
        if self.answer.is_none() && crate::supported::reads_response(outcome) {
            self.answer = self.draw_response(outcome)?;
        }
        Ok(())
    }

    /// The command's response, where `outcome` answers it ([`crate::mint::answers`]): the one
    /// [`Self::prepare`] drew, else drawn now ([`Self::draw_response`]). `None` for every other
    /// outcome.
    fn response(
        &self,
        outcome: &ResolvedOutcome,
    ) -> Result<Option<Map<String, Value>>, ExecuteError> {
        match &self.answer {
            Some(answer) if crate::mint::answers(outcome) => Ok(Some(answer.clone())),
            _ => self.draw_response(outcome),
        }
    }

    /// The command's response, where `outcome` answers it ([`crate::mint::answers`]): each
    /// declared field chosen from its type through the [`Generator`] ([`crate::mint::response`]).
    /// `None` for every other outcome.
    ///
    /// A one-time field's value is checked to appear in no other field and no field name of this
    /// answer, as ESS's interpreter checks the one it originates; a refusal names the field, never
    /// the value.
    fn draw_response(
        &self,
        outcome: &ResolvedOutcome,
    ) -> Result<Option<Map<String, Value>>, ExecuteError> {
        if !crate::mint::answers(outcome) {
            return Ok(None);
        }
        let command = self.spec.name.to_string();
        let mut response = Map::new();
        for field in &self.spec.response {
            let at = || {
                format!(
                    "`{command}.{}` in `{command}/{}`",
                    field.name,
                    outcome.name.as_str()
                )
            };
            let one_time = outcome.one_time_response.contains(&field.name);
            let choice =
                crate::mint::response(self.ir, &field.type_ref, one_time).map_err(|_| {
                    ExecuteError::Unsupported {
                        construct: format!(
                            "the response field {}, whose type the runtime does not choose",
                            at()
                        ),
                    }
                })?;
            let value = self
                .generator
                .choose(
                    &choice,
                    &Identity {
                        capability: IDENTITY_MINT,
                        command: self.command,
                        outcome: outcome.name.as_str(),
                        entity: &command,
                        field: &field.name,
                    },
                )
                .map_err(|error| {
                    ExecuteError::InvalidValue(format!(
                        "the response value minted for {} would be {error}",
                        at()
                    ))
                })?;
            crate::typed::validate(self.ir, &field.type_ref, &value).map_err(|_| {
                ExecuteError::InvalidValue(format!(
                    "the response value minted for {} is not a `{}`",
                    at(),
                    field.type_ref
                ))
            })?;
            response.insert(field.name.clone(), value);
        }
        for name in &outcome.one_time_response {
            let disclosed = match response.get(name) {
                Some(Value::String(text)) if !text.is_empty() => text,
                _ => {
                    return Err(ExecuteError::InvalidValue(format!(
                        "the one-time response field `{name}` of `{command}/{}` is not minted as \
                         text",
                        outcome.name.as_str()
                    )));
                }
            };
            if response.iter().any(|(other, value)| {
                other.contains(disclosed.as_str())
                    || (other != name && value.to_string().contains(disclosed.as_str()))
            }) {
                return Err(ExecuteError::InvalidValue(format!(
                    "the one-time response field `{name}` of `{command}/{}` would be disclosed \
                     beside its own field too",
                    outcome.name.as_str()
                )));
            }
        }
        Ok(Some(response))
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

/// Writes `sets`, an outcome's or an `affects:` entry's, into `fields`, reading the instance as it
/// was before. Each value is checked against the type its target declares, as ESS checks a sum or
/// a written value.
fn write(
    sources: &Sources<'_, '_>,
    sets: &[ResolvedPayloadField],
    fields: &mut BTreeMap<String, Value>,
) -> Result<(), ExecuteError> {
    let mut written = Vec::with_capacity(sets.len());
    for set in sets {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};

    use super::at_rest;
    use crate::ExecuteError;
    use crate::facts::ROWS_BUILT;
    use crate::store::Row;

    fn built() -> usize {
        ROWS_BUILT.with(std::cell::Cell::get)
    }

    fn row(state: &str, fields: Value) -> Row {
        let Value::Object(fields) = fields else {
            panic!("an object, not {fields}")
        };
        Row {
            state: state.to_owned(),
            fields: fields.into_iter().collect(),
        }
    }

    /// `tests/fixtures/adversary-affects`: `Account` declares no invariant, `Session` declares
    /// `hits <= 100`. Checking an `Account` row builds no facts; a `Session` row still does, and
    /// one that breaks the invariant is still refused.
    #[test]
    fn an_entity_without_invariants_builds_no_facts() {
        let ir = spindle_spec::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-affects"),
        )
        .expect("the fixture compiles")
        .ir;
        let before = built();
        for index in 0..240 {
            at_rest(
                &ir,
                "tenancy.accounts.Account",
                &json!(format!("a-{index}")),
                &row("Active", json!({"level": index})),
            )
            .expect("an account rests");
        }
        assert_eq!(
            built() - before,
            0,
            "facts built for an entity with no invariant"
        );

        let before = built();
        at_rest(
            &ir,
            "tenancy.accounts.Session",
            &json!("s"),
            &row("Live", json!({"account_id": "a-0", "hits": 3})),
        )
        .expect("a session within its invariant rests");
        assert_eq!(
            built() - before,
            1,
            "a declared invariant reads the row's facts"
        );
        let broken = at_rest(
            &ir,
            "tenancy.accounts.Session",
            &json!("s"),
            &row("Live", json!({"account_id": "a-0", "hits": 101})),
        );
        assert!(
            matches!(broken, Err(ExecuteError::BrokenInvariant { .. })),
            "{broken:?}"
        );
    }
}
