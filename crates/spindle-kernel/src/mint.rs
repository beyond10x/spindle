//! Identities and generated values the runtime mints.
//!
//! A `creates:` outcome whose new instance's identity is neither an input nor a payload source
//! (`{generated: true}`, or no source at all) leaves the identity to the runtime. Admission binds
//! that as the capability `identity.mint`; the kernel asks a [`Mint`] for the value, so a test can
//! make it deterministic. A `Uuid` identity, or a newtype of one without invariants, is minted as
//! a UUID; a `String` newtype whose prefix and alphabet admit a UUID's text, and whose invariants
//! read only `value.count` and hold for that text's length, as its prefix and a UUID's text
//! ([`identity`]). An entity invariant over the identity's length that no minted identity meets
//! refuses the command before it runs ([`identity_bound`]).
//!
//! Any other field an outcome marks `{generated: true}` (in `sets:`, a payload or an error, and
//! the `else:` of `{input: f, else: {generated: true}}`) is minted from its declared type
//! (`spindle_capability::mint::minted`, the rule derivation reads too): a `Uuid`, a `String`, an
//! `Integer`, a `Boolean`, a `Bytes` and a `List` from a UUID the same [`Mint`] answers, a
//! `Timestamp` from the clock the run is given. A `Boolean` is `true` where that UUID's last bit
//! is clear: ESS's interpreter mints `true` on an even tick of the counter it mints identities
//! from, and its UUID for tick `n` ends in `n`'s decimal digits (`interpret::values::mint`). A
//! `Bytes` is the one byte ESS mints on that tick, the tick's low byte, read from the same digits
//! where the UUID has ESS's tick form, and the UUID's own low byte otherwise ([`byte`]); a `List` is `[]`, as ESS mints every list, and still takes its draw, as ESS takes
//! a tick for it. So a [`Mint`] that answers those
//! UUIDs mints the values the interpreter does: the kernel draws in the interpreter's order
//! (`interpret::execute::take`), each published value a `{payload: <Event>.<field>}` reads
//! first, then a minted creation identity, then the other `sets:`, then the payloads.
//! That holds up to the first response field a command answers: the kernel draws response values
//! from the same [`Mint`], where the interpreter draws them from a counter of its own, so after a
//! returning outcome the two sequences no longer line up.
//! Each such field is minted once per command run ([`Generator`]), so
//! `{payload: <Event>.<field>}` in `sets:` stores what the event published.
//!
//! An outcome that returns the command's response ([`answers`]) has each declared `response:`
//! field minted by the same rule, an `Optional` one as its required type, after everything else
//! the outcome draws: ESS leaves every response value to the implementation. A one-time field
//! (`one_time_response:`) is a `String` and so a fresh UUID's text on every draw; it is minted
//! for the answer alone and written nowhere else.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedEffect, ResolvedEntity, ResolvedField, ResolvedInstance, ResolvedOutcome,
    ResolvedPayloadField, ResolvedPayloadValue, ResolvedTypeRef,
};
use ess_domain::Invariant;
use ess_domain::QualifiedName;
use serde_json::Value;
pub(crate) use spindle_capability::mint::{Minted, absent, minted};
use spindle_capability::mint::{candidate, holds_for, length_only};
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

/// How a minted creation identity of type `kind` is spelled, by [`minted`], the rule for
/// generated fields: a `Uuid` or a newtype of one without invariants, or a `String` newtype whose
/// prefix, alphabet and length invariants a minted text satisfies. `None` for every other type,
/// among them a bare `String` and a `Uuid` newtype that declares an invariant.
pub(crate) fn identity(ir: &EssIr, kind: &ResolvedTypeRef) -> Option<Minted> {
    match minted(ir, kind, false)? {
        Minted::Uuid => Some(Minted::Uuid),
        text @ Minted::Text { .. } if matches!(kind, ResolvedTypeRef::Declared { .. }) => {
            Some(text)
        }
        _ => None,
    }
}

/// The first invariant of `entity` that reads only its identity's length and that no identity
/// minted as `shape` meets: every minted identity under one shape has one length, so ESS's
/// verdict on [`candidate`] is its verdict on each.
pub(crate) fn identity_bound<'ir>(
    entity: &'ir ResolvedEntity,
    shape: &Minted,
) -> Option<&'ir Invariant> {
    let text = match shape {
        Minted::Text { prefix } => candidate(prefix.as_deref()),
        _ => candidate(None),
    };
    let subject = entity.identity.name.as_str();
    entity
        .invariants
        .iter()
        .find(|invariant| length_only(invariant, subject) && !holds_for(invariant, subject, &text))
}

/// Whether `outcome` answers the command's response: it returns it (`returns: true`) and reports
/// no error, as ESS's interpreter decides it (`interpret::protected::response`).
pub(crate) fn answers(outcome: &ResolvedOutcome) -> bool {
    outcome.returns && outcome.error.is_none()
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

    /// A fresh value of `shape` for `identity`.
    ///
    /// # Errors
    /// The reason, from [`value`], where the value cannot be spelled.
    pub(crate) fn mint(&self, shape: &Minted, identity: &Identity<'_>) -> Result<Value, String> {
        value(shape, || self.uuid(identity), self.now)
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
         `String`, an `Integer`, a `Boolean`, a `Bytes`, a `List` or a `Timestamp`, or a newtype \
         of one without invariants"
    )
}

/// The refusal of the error field `field` of `error` that the refusing outcome gives no source,
/// no row it is answered for holds and whose type the runtime does not mint
/// ([`spindle_capability::mint::Unsourced::Unfilled`]).
pub(crate) fn unfilled(error: &str, field: &ResolvedField) -> String {
    format!(
        "the error field `{error}.{}` of type `{}`, which the outcome gives no source and no row \
         it is answered for holds; the runtime mints a `Uuid`, a `String`, an `Integer`, a \
         `Boolean`, a `Bytes`, a `List` or a `Timestamp`, or a newtype of one without invariants",
        field.name, field.type_ref
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
        Minted::Boolean => Ok(Value::Bool(uuid().as_u128() & 1 == 0)),
        Minted::Bytes => Ok(Value::String(base64_byte(byte(uuid())))),
        Minted::List => {
            uuid();
            Ok(Value::Array(Vec::new()))
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

/// The one byte a generated `Bytes` holds, drawn from `uuid`.
///
/// A UUID in the form ESS's interpreter mints on a tick (`00000000-0000-4000-8000-<tick, 12
/// decimal digits>`, `interpret::values::mint`) gives the byte ESS mints on that tick: the
/// tick's low byte, a tick past `u32::MAX` as `u32::MAX` (`witness::primitive_value`). Any other
/// UUID, a random version 4 one among them, gives its own low byte, so random UUIDs give a
/// spread of bytes.
fn byte(uuid: Uuid) -> u8 {
    let bits = uuid.as_u128();
    let low = bits.to_le_bytes()[0];
    if bits >> 48 != TICK_FORM {
        return low;
    }
    let mut tick: u64 = 0;
    for digit in (0..12).rev() {
        let digit = (bits >> (4 * digit)) & 0xf;
        if digit > 9 {
            return low;
        }
        tick = tick * 10 + u64::try_from(digit).unwrap_or_default();
    }
    u32::try_from(tick).unwrap_or(u32::MAX).to_le_bytes()[0]
}

/// The leading 80 bits of every UUID ESS's interpreter mints on a tick:
/// `00000000-0000-4000-8000`.
const TICK_FORM: u128 = 0x0000_0000_0000_4000_8000;

/// `value` as padded base64, the spelling a `Bytes` value takes: ESS's one-byte witness
/// (`witness::base64_byte`).
fn base64_byte(value: u8) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let high = char::from(ALPHABET[usize::from(value >> 2)]);
    let low = char::from(ALPHABET[usize::from((value & 0b11) << 4)]);
    format!("{high}{low}==")
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

/// Version 4 UUIDs drawn from a seed: two mints made from one seed answer the same UUIDs in the
/// same order, so the same commands mint the same identities. Each UUID's random bits are the
/// next 128 bits of a `SplitMix64` sequence started at the seed; not for anything an attacker may
/// guess. A UUID it was told is [held](Seeded::avoiding) is skipped and the next one drawn, so it
/// never mints an identity a seeded instance already holds and stays deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seeded {
    state: u64,
    held: BTreeSet<Uuid>,
}

impl Seeded {
    /// A mint whose UUIDs follow from `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed,
            held: BTreeSet::new(),
        }
    }

    /// This mint, never answering a UUID among `held`.
    #[must_use]
    pub fn avoiding(mut self, held: impl IntoIterator<Item = Uuid>) -> Self {
        self.held.extend(held);
        self
    }

    fn draw(&mut self) -> Uuid {
        let high = self.next().to_be_bytes();
        let low = self.next().to_be_bytes();
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&high);
        bytes[8..].copy_from_slice(&low);
        uuid::Builder::from_random_bytes(bytes).into_uuid()
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^ (mixed >> 31)
    }
}

impl Mint for Seeded {
    fn uuid(&mut self, _identity: &Identity<'_>) -> Uuid {
        let mut drawn = self.draw();
        for _ in 0..=self.held.len() {
            if !self.held.contains(&drawn) {
                break;
            }
            drawn = self.draw();
        }
        drawn
    }
}
