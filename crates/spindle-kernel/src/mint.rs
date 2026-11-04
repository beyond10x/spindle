//! Identities and generated values the runtime mints.
//!
//! A `creates:` outcome whose new instance's identity is neither an input nor a payload source
//! (`{generated: true}`, or no source at all) leaves the identity to the runtime. Admission binds
//! that as the capability `identity.mint`; the kernel asks a [`Mint`] for the value, so a test can
//! make it deterministic. Only a `Uuid` identity, or a newtype of one, is minted.
//!
//! Any other field an outcome marks `{generated: true}` (in `sets:`, a payload or an error, and
//! the `else:` of `{input: f, else: {generated: true}}`) is minted from its declared type
//! (`spindle_capability::mint::minted`, the rule derivation reads too): a `Uuid`, a `String` and
//! an `Integer` from a UUID the same [`Mint`] answers, a `Timestamp` from the clock the run is
//! given. Each such field is minted once per command run ([`Generator`]), so
//! `{payload: <Event>.<field>}` in `sets:` stores what the event published.

use std::cell::RefCell;
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBody, ResolvedEffect, ResolvedEntity, ResolvedField, ResolvedInstance, ResolvedOutcome,
    ResolvedPayloadField, ResolvedPayloadValue, ResolvedTypeRef,
};
use ess_domain::QualifiedName;
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};
use serde_json::Value;
pub(crate) use spindle_capability::mint::{Minted, absent, minted};
pub use uuid::Uuid;

use crate::ExecuteError;

/// The capability a minted identity is bound through, as admission derives it.
pub(crate) const IDENTITY_MINT: &str = "identity.mint";

/// The entity and identity field of the new instance `outcome` creates, where the outcome leaves
/// the identity to the runtime: a `creates:` whose published identity field has no payload source
/// or the source `{generated: true}`. `None` for every other outcome, among them a creation whose
/// identity is an input or another payload source.
pub(crate) fn slot<'ir>(
    ir: &'ir EssIr,
    outcome: &'ir ResolvedOutcome,
) -> Option<(&'ir ResolvedEntity, &'ir ResolvedField)> {
    let subject = outcome.subject.as_ref()?;
    let (ResolvedEffect::Creates, ResolvedInstance::Observed { event, field }) =
        (&subject.effect, &subject.instance)
    else {
        return None;
    };
    let source = outcome
        .payload
        .iter()
        .filter(|payload| payload.event == *event)
        .flat_map(|payload| &payload.fields)
        .find(|source| source.target == field.name);
    match source.map(|source| &source.value) {
        None | Some(ResolvedPayloadValue::Generated { .. }) => {
            Some((ir.entity(&subject.entity), field))
        }
        Some(_) => None,
    }
}

/// Whether a value of `kind` is a `Uuid`: the primitive, or a newtype that wraps one.
pub(crate) fn is_uuid(ir: &EssIr, kind: &ResolvedTypeRef) -> bool {
    let mut kind = kind;
    for _ in 0..=MAX_TYPE_DEPTH {
        match kind {
            ResolvedTypeRef::Primitive { name } => return *name == Primitive::Uuid,
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype { of, .. } => kind = of,
                _ => return false,
            },
            _ => return false,
        }
    }
    false
}

/// The low 53 bits: the integers every JSON reader holds exactly.
const INTEGER_BITS: u128 = (1 << 53) - 1;

/// The values one command run mints for its generated fields, each minted once: a field read
/// twice, in a payload and again through `{payload: <Event>.<field>}` in `sets:`, reads the same
/// value.
pub(crate) struct Generator<'m> {
    mint: RefCell<&'m mut dyn Mint>,
    /// The civil time a generated `Timestamp` is stamped with.
    now: &'m dyn Fn() -> SystemTime,
    command: &'m str,
    /// Each field minted, by its place in the model, and the value it took.
    minted: RefCell<Vec<(*const ResolvedPayloadField, Option<Value>)>>,
}

impl<'m> Generator<'m> {
    pub(crate) fn new(
        mint: &'m mut dyn Mint,
        now: &'m dyn Fn() -> SystemTime,
        command: &'m str,
    ) -> Self {
        Self {
            mint: RefCell::new(mint),
            now,
            command,
            minted: RefCell::new(Vec::new()),
        }
    }

    /// A fresh UUID for `identity`.
    pub(crate) fn uuid(&self, identity: &Identity<'_>) -> Uuid {
        self.mint.borrow_mut().uuid(identity)
    }

    /// The value the generated `field`, of `owner` in `outcome`, holds in this run: minted on the
    /// first read, the same value on every later one; `None` where the mint leaves it absent.
    ///
    /// # Errors
    /// [`ExecuteError::Unsupported`] for a type the runtime does not mint, which
    /// [`crate::supported`] refuses before a command runs, and [`ExecuteError::InvalidValue`]
    /// where the minted value is not a value of the field's type.
    pub(crate) fn generated(
        &self,
        ir: &EssIr,
        outcome: &str,
        owner: &QualifiedName,
        field: &ResolvedPayloadField,
        present: bool,
    ) -> Result<Option<Value>, ExecuteError> {
        let place = std::ptr::from_ref(field);
        if let Some((_, held)) = self
            .minted
            .borrow()
            .iter()
            .find(|(minted, _)| std::ptr::eq(*minted, place))
        {
            return Ok(held.clone());
        }
        let value = if absent(&field.target_type, present) {
            None
        } else {
            let shape = minted(ir, &field.target_type, present).ok_or_else(|| {
                ExecuteError::Unsupported {
                    construct: unmintable(&field.target, &field.target_type),
                }
            })?;
            let owner = owner.to_string();
            let value = value(
                &shape,
                || {
                    self.uuid(&Identity {
                        capability: IDENTITY_MINT,
                        command: self.command,
                        outcome,
                        entity: &owner,
                        field: &field.target,
                    })
                },
                self.now,
            )
            .map_err(|error| {
                ExecuteError::InvalidValue(format!(
                    "the value minted for `{owner}.{}` in `{}/{outcome}` would be {error}",
                    field.target, self.command
                ))
            })?;
            crate::typed::validate(ir, &field.target_type, &value).map_err(|error| {
                ExecuteError::InvalidValue(format!(
                    "the value minted for `{owner}.{}` in `{}/{outcome}` would hold {error}",
                    field.target, self.command
                ))
            })?;
            Some(value)
        };
        self.minted.borrow_mut().push((place, value.clone()));
        Ok(value)
    }
}

/// The refusal of `{generated: true}` on `field`, whose type the runtime does not mint.
pub(crate) fn unmintable(field: &str, kind: &ResolvedTypeRef) -> String {
    format!(
        "`{{generated: true}}` on `{field}` of type `{kind}`; the runtime mints a `Uuid`, a \
         `String`, an `Integer` or a `Timestamp`, or a newtype of one without invariants"
    )
}

/// The value one [`Minted`] shape takes, the UUID it needs drawn from `uuid` and the time from
/// `now`; the reason where `now` is a time RFC 3339 does not spell (before the Unix epoch's
/// representable range or past year 9999).
pub(crate) fn value(
    minted: &Minted,
    uuid: impl FnOnce() -> Uuid,
    now: &dyn Fn() -> SystemTime,
) -> Result<Value, String> {
    match minted {
        Minted::Uuid => Ok(Value::String(uuid().hyphenated().to_string())),
        Minted::Text { prefix } => Ok(Value::String(format!(
            "{}{}",
            prefix.as_deref().unwrap_or_default(),
            uuid().hyphenated()
        ))),
        Minted::Integer => {
            let low = uuid().as_u128() & INTEGER_BITS;
            u64::try_from(low)
                .map(Value::from)
                .map_err(|_| format!("{low} is past the integers a JSON reader holds"))
        }
        Minted::Timestamp => {
            let at = now();
            let unrepresentable = || format!("{at:?}, a time no RFC 3339 instant names");
            let since = match at.duration_since(SystemTime::UNIX_EPOCH) {
                Ok(after) => time::Duration::try_from(after).ok(),
                Err(before) => time::Duration::try_from(before.duration())
                    .ok()
                    .and_then(|before| before.checked_neg()),
            }
            .ok_or_else(unrepresentable)?;
            time::OffsetDateTime::UNIX_EPOCH
                .checked_add(since)
                .filter(|stamp| (0..=9999).contains(&stamp.year()))
                .ok_or_else(unrepresentable)?
                .format(&time::format_description::well_known::Rfc3339)
                .map(Value::String)
                .map_err(|_| unrepresentable())
        }
    }
}

/// One value the runtime mints: a creation's identity, or a field an outcome marks
/// `{generated: true}`.
#[derive(Debug)]
pub struct Identity<'a> {
    /// The capability that answers it: `identity.mint`, as derived for admission.
    pub capability: &'static str,
    /// The command.
    pub command: &'a str,
    /// The creating outcome, or the outcome whose field is minted.
    pub outcome: &'a str,
    /// The entity of the new instance; for a generated field, the entity, event or error whose
    /// field it is.
    pub entity: &'a str,
    /// The identity field, as the entity names it, or the generated field (a struct member by
    /// its own name).
    pub field: &'a str,
}

/// Mints the identity of a new instance.
pub trait Mint {
    /// A fresh `Uuid` for `identity`. The kernel refuses one that an instance already holds.
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid;
}

impl<F> Mint for F
where
    F: FnMut(&Identity<'_>) -> Uuid,
{
    fn uuid(&mut self, identity: &Identity<'_>) -> Uuid {
        self(identity)
    }
}

/// Random version 4 UUIDs, from the operating system's random source.
#[derive(Debug, Default, Clone, Copy)]
pub struct UuidV4;

impl UuidV4 {
    /// The implementation name an environment binds `identity.mint` to for this mint.
    pub const IMPLEMENTATION: &str = "uuid-v4";
}

impl Mint for UuidV4 {
    fn uuid(&mut self, _identity: &Identity<'_>) -> Uuid {
        Uuid::new_v4()
    }
}
