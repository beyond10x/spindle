//! Identities and generated values the runtime mints.
//!
//! A `creates:` outcome whose new instance's identity is neither an input nor a payload source
//! (`{generated: true}`, or no source at all) leaves the identity to the runtime. Admission binds
//! that as the capability `identity.mint`; the kernel asks a [`Mint`] for the value, so a test can
//! make it deterministic. Only a `Uuid` identity, or a newtype of one, is minted.
//!
//! Any other field an outcome marks `{generated: true}` (in `sets:`, a payload or an error, and
//! the `else:` of `{input: f, else: {generated: true}}`) is minted from its declared type
//! ([`minted`]): a `Uuid`, a `String` and an `Integer` from a UUID the same [`Mint`] answers, a
//! `Timestamp` from the clock the run is given. Each such field is minted once per command run
//! ([`Generator`]), so `{payload: <Event>.<field>}` in `sets:` stores what the event published.

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

/// What a field the runtime mints, `{generated: true}` on anything but a minted creation identity,
/// is minted as: the shape of its declared type underneath every newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Minted {
    /// A `Uuid`: a fresh UUID from the [`Mint`].
    Uuid,
    /// A `String`: a fresh UUID's text from the [`Mint`], after the prefix a newtype declares.
    Text {
        /// The prefix the outermost newtype that declares one requires.
        prefix: Option<String>,
    },
    /// An `Integer`: the low 53 bits of a fresh UUID from the [`Mint`], so it is exact in every
    /// JSON reader.
    Integer,
    /// A `Timestamp`: the civil time the run is given, in UTC, RFC 3339.
    Timestamp,
}

/// The characters a minted text holds besides its prefix: a hyphenated lower-case UUID's.
const MINTED_TEXT: &str = "0123456789abcdef-";

/// The low 53 bits: the integers every JSON reader holds exactly.
const INTEGER_BITS: u128 = (1 << 53) - 1;

/// How a field of `kind` is minted, where the runtime mints it; `None` where it does not.
///
/// The runtime mints a `Uuid`, a `String`, an `Integer` and a `Timestamp`, and a newtype of one
/// that declares no invariant. A `String` newtype's alphabet must admit the minted text and its
/// prefix, and a newtype of anything but a `String` may declare neither. `present` is the mint
/// of `{generated: true, present: true}`: an `Optional` field is minted as its required type.
/// Without it an `Optional` field stays absent, as ESS's interpreter leaves it
/// (`interpret::execute::mint`), so it is minted as nothing.
pub(crate) fn minted(ir: &EssIr, kind: &ResolvedTypeRef, present: bool) -> Option<Minted> {
    let mut kind = if present { kind.required() } else { kind };
    let mut prefix: Option<&str> = None;
    let mut alphabets: Vec<&str> = Vec::new();
    for _ in 0..=MAX_TYPE_DEPTH {
        match kind {
            ResolvedTypeRef::Primitive { name } => {
                let minted = match name {
                    Primitive::Uuid => Minted::Uuid,
                    Primitive::String => Minted::Text {
                        prefix: prefix.map(str::to_owned),
                    },
                    Primitive::Integer => Minted::Integer,
                    Primitive::Timestamp => Minted::Timestamp,
                    _ => return None,
                };
                let text = matches!(minted, Minted::Text { .. });
                if !text && (prefix.is_some() || !alphabets.is_empty()) {
                    return None;
                }
                let admitted = |character: char| {
                    alphabets
                        .iter()
                        .all(|alphabet| alphabet.contains(character))
                };
                return prefix
                    .unwrap_or_default()
                    .chars()
                    .chain(MINTED_TEXT.chars())
                    .all(admitted)
                    .then_some(minted);
            }
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype {
                    of,
                    alphabet,
                    prefix: declared,
                    invariants,
                } if invariants.is_empty() => {
                    if let Some(declared) = declared {
                        match prefix {
                            None => prefix = Some(declared),
                            Some(outer) if outer.starts_with(declared.as_str()) => {}
                            Some(_) => return None,
                        }
                    }
                    if let Some(alphabet) = alphabet {
                        alphabets.push(alphabet);
                    }
                    kind = of;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}

/// Whether a `{generated: true}` over `kind` leaves the field absent: an `Optional` field without
/// `present: true`, the value that claims least, as ESS's interpreter mints it.
pub(crate) fn absent(kind: &ResolvedTypeRef, present: bool) -> bool {
    !present && kind.is_optional()
}

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
            );
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
/// `now`.
pub(crate) fn value(
    minted: &Minted,
    uuid: impl FnOnce() -> Uuid,
    now: &dyn Fn() -> SystemTime,
) -> Value {
    match minted {
        Minted::Uuid => Value::String(uuid().hyphenated().to_string()),
        Minted::Text { prefix } => Value::String(format!(
            "{}{}",
            prefix.as_deref().unwrap_or_default(),
            uuid().hyphenated()
        )),
        Minted::Integer => {
            let low = uuid().as_u128() & INTEGER_BITS;
            Value::from(u64::try_from(low).unwrap_or_default())
        }
        Minted::Timestamp => Value::String(
            time::OffsetDateTime::from(now())
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ),
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
