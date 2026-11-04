//! Identities the runtime mints.
//!
//! A `creates:` outcome whose new instance's identity is neither an input nor a payload source
//! (`{generated: true}`, or no source at all) leaves the identity to the runtime. Admission binds
//! that as the capability `identity.mint`; the kernel asks a [`Mint`] for the value, so a test can
//! make it deterministic. Only a `Uuid` identity, or a newtype of one, is minted.

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBody, ResolvedEffect, ResolvedEntity, ResolvedField, ResolvedInstance, ResolvedOutcome,
    ResolvedPayloadValue, ResolvedTypeRef,
};
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};
pub use uuid::Uuid;

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

/// The identity one creation asks to have minted.
#[derive(Debug)]
pub struct Identity<'a> {
    /// The capability that answers it: `identity.mint`, as derived for admission.
    pub capability: &'static str,
    /// The command.
    pub command: &'a str,
    /// The creating outcome.
    pub outcome: &'a str,
    /// The entity of the new instance.
    pub entity: &'a str,
    /// The identity field, as the entity names it.
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
