//! Which generated values the runtime mints, and through which capability.
//!
//! One rule, read by derivation ([`crate::derive`]) and by the kernel that mints: a field an
//! outcome marks `{generated: true}`, other than a minted creation identity, is minted from its
//! declared type where [`minted`] answers a shape, left absent where [`absent`] holds, and
//! refused by name otherwise. Derivation therefore names a capability exactly where the kernel
//! mints.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
use ess_domain::Invariant;
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};
use ess_primitives::facts::{FactStore, FactValue};
use ess_primitives::predicate::Truth;

use crate::Capability;

/// What a generated field is minted as: the shape of its declared type underneath every newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Minted {
    /// A `Uuid`: a fresh UUID.
    Uuid,
    /// A `String`: a fresh UUID's text, after the prefix a newtype declares.
    Text {
        /// The prefix the outermost newtype that declares one requires.
        prefix: Option<String>,
    },
    /// An `Integer`: drawn from a fresh UUID.
    Integer,
    /// A `Boolean`: drawn from a fresh UUID, as ESS's interpreter draws one from the counter it
    /// mints identities from (`interpret::values::mint`).
    Boolean,
    /// A `Bytes`: one byte drawn from a fresh UUID, as ESS's interpreter draws one from the
    /// counter it mints identities from (`interpret::values::mint`).
    Bytes,
    /// A `List` of anything: the empty list, as ESS's interpreter mints every list, from a draw
    /// of its own on that counter (`interpret::values::mint`).
    List,
    /// A `Timestamp`: the clock's civil time.
    Timestamp,
}

impl Minted {
    /// The capability that supplies a value of this shape: `clock` for a `Timestamp`,
    /// `identity.mint` for the rest.
    pub fn capability(&self) -> &'static str {
        match self {
            Self::Timestamp => Capability::CLOCK,
            Self::Uuid
            | Self::Text { .. }
            | Self::Integer
            | Self::Boolean
            | Self::Bytes
            | Self::List => Capability::IDENTITY_MINT,
        }
    }
}

/// The characters a minted text holds besides its prefix: a hyphenated lower-case UUID's.
pub const MINTED_TEXT: &str = "0123456789abcdef-";

/// How a field of `kind` is minted, where the runtime mints it; `None` where it does not.
///
/// The runtime mints a `Uuid`, a `String`, an `Integer`, a `Boolean`, a `Bytes`, a `List` of
/// any element type and a `Timestamp`, and a newtype of one that declares no invariant. A `String` newtype's alphabet must admit the minted
/// text and its prefix, where nested prefixes extend one another the longest; its invariants may read only
/// `value.count` ([`length_only`]), and ESS must find each true of [`candidate`]. A newtype of
/// anything but a `String` may declare none of these. `present` is the mint
/// of `{generated: true, present: true}`: an `Optional` field is minted as its required type.
/// Without it an `Optional` field stays absent ([`absent`]), so it is minted as nothing.
pub fn minted(ir: &EssIr, kind: &ResolvedTypeRef, present: bool) -> Option<Minted> {
    let mut kind = if present { kind.required() } else { kind };
    let mut prefix: Option<&str> = None;
    let mut alphabets: Vec<&str> = Vec::new();
    let mut bounds: Vec<&Invariant> = Vec::new();
    for _ in 0..=MAX_TYPE_DEPTH {
        match kind {
            ResolvedTypeRef::Primitive { name } => {
                let minted = match name {
                    Primitive::Uuid => Minted::Uuid,
                    Primitive::String => Minted::Text {
                        prefix: prefix.map(str::to_owned),
                    },
                    Primitive::Integer => Minted::Integer,
                    Primitive::Boolean => Minted::Boolean,
                    Primitive::Bytes => Minted::Bytes,
                    Primitive::Timestamp => Minted::Timestamp,
                    _ => return None,
                };
                let text = matches!(minted, Minted::Text { .. });
                if !text && (prefix.is_some() || !alphabets.is_empty() || !bounds.is_empty()) {
                    return None;
                }
                let admitted = |character: char| {
                    alphabets
                        .iter()
                        .all(|alphabet| alphabet.contains(character))
                };
                let candidate = candidate(prefix);
                return (candidate.chars().all(admitted)
                    && bounds
                        .iter()
                        .all(|bound| holds_for(bound, "value", &candidate)))
                .then_some(minted);
            }
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype {
                    of,
                    alphabet,
                    prefix: declared,
                    invariants,
                } if invariants
                    .iter()
                    .all(|invariant| length_only(invariant, "value")) =>
                {
                    bounds.extend(invariants);
                    if let Some(declared) = declared {
                        match prefix {
                            None => prefix = Some(declared),
                            Some(outer) if outer.starts_with(declared.as_str()) => {}
                            Some(outer) if declared.starts_with(outer) => prefix = Some(declared),
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
            ResolvedTypeRef::List { .. } => {
                return (prefix.is_none() && alphabets.is_empty() && bounds.is_empty())
                    .then_some(Minted::List);
            }
            _ => return None,
        }
    }
    None
}

/// A UUID's text, the representative a check of every minted text reads: each minted text is its
/// prefix and a hyphenated UUID, so it has this length and draws from [`MINTED_TEXT`].
const REPRESENTATIVE: &str = "00000000-0000-4000-8000-000000000000";

/// A minted text as a check of every minted text reads it: `prefix`, then a UUID's text.
pub fn candidate(prefix: Option<&str>) -> String {
    format!("{}{REPRESENTATIVE}", prefix.unwrap_or_default())
}

/// Whether `invariant` reads nothing but the length of the text at `subject` (`value.count`
/// for a newtype, `<identity>.count` for an entity): then it holds for every minted text exactly
/// where it holds for [`candidate`], since every minted text under one prefix has one length.
pub fn length_only(invariant: &Invariant, subject: &str) -> bool {
    let predicate = &invariant.predicate;
    let paths = predicate.fact_paths();
    !paths.is_empty()
        && paths
            .iter()
            .all(|path| path.segments() == [subject, "count"])
        && !predicate.reads_call()
        && !predicate.reads_utf8_bytes()
        && !predicate.reads_window()
}

/// Whether ESS evaluates `invariant` as true with the text `text` bound at `subject`.
pub fn holds_for(invariant: &Invariant, subject: &str, text: &str) -> bool {
    let mut facts = FactStore::new();
    facts.set_path(subject, FactValue::Text(text.to_owned()));
    invariant.predicate.evaluate(&facts) == Truth::True
}

/// Whether a `{generated: true}` over `kind` leaves the field absent: an `Optional` field without
/// `present: true`, the value that claims least, as ESS's interpreter mints it
/// (`interpret::execute::mint`).
pub fn absent(kind: &ResolvedTypeRef, present: bool) -> bool {
    !present && kind.is_optional()
}
