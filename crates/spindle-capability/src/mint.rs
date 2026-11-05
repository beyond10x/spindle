//! Which generated values the runtime mints, and through which capability.
//!
//! One rule, read by derivation ([`crate::derive`]) and by the kernel that mints: a field an
//! outcome marks `{generated: true}`, other than a minted creation identity, is minted from its
//! declared type where [`minted`] answers a shape, left absent where [`absent`] holds, and
//! refused by name otherwise. Derivation therefore names a capability exactly where the kernel
//! mints.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
use ess_domain::types::{MAX_TYPE_DEPTH, Primitive};

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
    /// A `Timestamp`: the clock's civil time.
    Timestamp,
}

impl Minted {
    /// The capability that supplies a value of this shape: `clock` for a `Timestamp`,
    /// `identity.mint` for the rest.
    pub fn capability(&self) -> &'static str {
        match self {
            Self::Timestamp => Capability::CLOCK,
            Self::Uuid | Self::Text { .. } | Self::Integer | Self::Boolean => {
                Capability::IDENTITY_MINT
            }
        }
    }
}

/// The characters a minted text holds besides its prefix: a hyphenated lower-case UUID's.
pub const MINTED_TEXT: &str = "0123456789abcdef-";

/// How a field of `kind` is minted, where the runtime mints it; `None` where it does not.
///
/// The runtime mints a `Uuid`, a `String`, an `Integer`, a `Boolean` and a `Timestamp`, and a
/// newtype of one that declares no invariant. A `String` newtype's alphabet must admit the
/// minted text and its prefix, and a newtype of anything but a `String` may declare neither.
/// `present` is the mint of `{generated: true, present: true}`: an `Optional` field is minted as
/// its required type. Without it an `Optional` field stays absent ([`absent`]), so it is minted
/// as nothing.
pub fn minted(ir: &EssIr, kind: &ResolvedTypeRef, present: bool) -> Option<Minted> {
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
                    Primitive::Boolean => Minted::Boolean,
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
/// `present: true`, the value that claims least, as ESS's interpreter mints it
/// (`interpret::execute::mint`).
pub fn absent(kind: &ResolvedTypeRef, present: bool) -> bool {
    !present && kind.is_optional()
}
