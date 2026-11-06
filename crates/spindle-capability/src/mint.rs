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
use ess_primitives::node::Node;
use ess_primitives::predicate::{Operand, Predicate, Truth};

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

/// How the runtime builds a value it chooses of a type [`minted`] does not answer: a response
/// field the command returns, or a `{generated: true}` field ([`generated`]). The structure follows
/// ESS's interpreter (`interpret::response::base`): a record's members each by the same rules, a
/// union's first variant, one map entry, an enum's variant drawn as its input witness cycles them,
/// and a constrained newtype's literal. The literal is Spindle's choice among the literals the
/// newtype's invariants name, the first that ESS's predicates find true of it ([`admits`]); the
/// kernel then holds the whole value to ESS's type check.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// A leaf [`minted`] answers.
    Mint(Minted),
    /// A constrained newtype: a literal its invariants, or those of the type it wraps, name.
    Literal(FactValue),
    /// An enum: one of its variants, drawn through `identity.mint`.
    Enum(Vec<String>),
    /// A record: each member, an `Optional` one as its required type.
    Record(Vec<(String, Choice)>),
    /// A union's first variant by tag value: the tag, and its content under `value` (`content`
    /// where the tag itself is named `value`), none for a unit variant.
    Variant {
        /// The tag field.
        tag: String,
        /// The variant's tag value.
        label: String,
        /// The content key and the content.
        content: Option<(&'static str, Box<Choice>)>,
    },
    /// A map of one entry: a key minted as text, and its value.
    Entry {
        /// The key, a [`Minted::Uuid`] or [`Minted::Text`].
        key: Minted,
        /// The value.
        value: Box<Choice>,
    },
}

impl Choice {
    /// The capabilities the parts the runtime draws need, each once, in the order they are first
    /// drawn: `clock` for a `Timestamp`, `identity.mint` for every other minted leaf, an enum's
    /// variant among them. A literal draws nothing.
    pub fn capabilities(&self) -> Vec<&'static str> {
        let mut needed = Vec::new();
        self.collect(&mut needed);
        needed
    }

    fn collect(&self, needed: &mut Vec<&'static str>) {
        let mut need = |capability: &'static str| {
            if !needed.contains(&capability) {
                needed.push(capability);
            }
        };
        match self {
            Self::Mint(minted) => need(minted.capability()),
            Self::Enum(_) => need(Capability::IDENTITY_MINT),
            Self::Literal(_) | Self::Variant { content: None, .. } => {}
            Self::Record(members) => {
                for (_, member) in members {
                    member.collect(needed);
                }
            }
            Self::Variant {
                content: Some((_, content)),
                ..
            } => content.collect(needed),
            Self::Entry { key, value } => {
                need(key.capability());
                value.collect(needed);
            }
        }
    }

    /// Whether any part is a response field's alone: a constrained newtype's literal or an
    /// enum's variant.
    pub fn response_only(&self) -> bool {
        match self {
            Self::Literal(_) | Self::Enum(_) => true,
            Self::Mint(_) | Self::Variant { content: None, .. } => false,
            Self::Record(members) => members.iter().any(|(_, member)| member.response_only()),
            Self::Variant {
                content: Some((_, content)),
                ..
            } => content.response_only(),
            Self::Entry { value, .. } => value.response_only(),
        }
    }
}

/// How the runtime mints a `{generated: true}` field of `kind`: by [`minted`], or a record, union
/// or map of such leaves by [`chosen`]. Never through a constrained newtype's literal or an enum,
/// which only a response field takes here ([`Choice::response_only`]).
pub fn generated(ir: &EssIr, kind: &ResolvedTypeRef, present: bool) -> Option<Choice> {
    minted(ir, kind, present)
        .map(Choice::Mint)
        .or_else(|| chosen(ir, kind).filter(|choice| !choice.response_only()))
}

/// Why the runtime does not answer a response field: the kernel's refusal by name, and the field
/// derivation names no capability for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unanswered {
    /// No [`chosen`] value of the field's type.
    Unchosen,
    /// A one-time field (`one_time_response:`) whose value is not minted as text.
    OneTimeNotText,
}

/// How the runtime answers a response field of `kind`, where `one_time` says the outcome lists
/// it in `one_time_response:`: the one rule the kernel answers and refuses by and derivation
/// names capabilities by.
///
/// # Errors
/// [`Unanswered`], where the kernel refuses the field by name before the command runs.
pub fn response(ir: &EssIr, kind: &ResolvedTypeRef, one_time: bool) -> Result<Choice, Unanswered> {
    match chosen(ir, kind) {
        None => Err(Unanswered::Unchosen),
        Some(choice @ Choice::Mint(Minted::Text { .. })) => Ok(choice),
        Some(_) if one_time => Err(Unanswered::OneTimeNotText),
        Some(choice) => Ok(choice),
    }
}

/// How the runtime chooses a value of `kind`, an `Optional` one as its required type; `None`
/// where it does not, and then the kernel refuses the field by name and derivation names no
/// capability for it: a record that declares invariants, a record that holds itself, a
/// constrained newtype neither whose literals nor its wrapped type's are a value of it, a map
/// whose key is not minted as text, and every primitive [`minted`] does not answer.
pub fn chosen(ir: &EssIr, kind: &ResolvedTypeRef) -> Option<Choice> {
    choose(ir, kind, 0)
}

fn choose(ir: &EssIr, kind: &ResolvedTypeRef, depth: usize) -> Option<Choice> {
    if depth > MAX_TYPE_DEPTH {
        return None;
    }
    let kind = kind.required();
    if let Some(minted) = minted(ir, kind, true) {
        return Some(Choice::Mint(minted));
    }
    match kind {
        ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
            ResolvedBody::Struct { fields, invariants } if invariants.is_empty() => fields
                .iter()
                .map(|member| {
                    Some((
                        member.name.clone(),
                        choose(ir, &member.type_ref, depth + 1)?,
                    ))
                })
                .collect::<Option<_>>()
                .map(Choice::Record),
            ResolvedBody::Union { tag, variants } => {
                let (label, variant) = variants.iter().next()?;
                let content = match variant {
                    Some(variant) => Some((
                        if tag == "value" { "content" } else { "value" },
                        Box::new(choose(ir, variant, depth + 1)?),
                    )),
                    None => None,
                };
                Some(Choice::Variant {
                    tag: tag.clone(),
                    label: label.clone(),
                    content,
                })
            }
            ResolvedBody::Newtype {
                of,
                alphabet: None,
                prefix: None,
                invariants,
            } if invariants.is_empty() => choose(ir, of, depth + 1),
            ResolvedBody::Newtype { of, invariants, .. } => {
                let mut candidates = Vec::new();
                for invariant in invariants {
                    literals(&invariant.predicate, &mut candidates);
                }
                if let Some(found) = candidates
                    .into_iter()
                    .find(|candidate| admits(ir, kind, candidate))
                {
                    return Some(Choice::Literal(found));
                }
                // As `base` does: the wrapped type's value, held to this newtype's rules too.
                match choose(ir, of, depth + 1)? {
                    Choice::Literal(wrapped) if admits(ir, kind, &wrapped) => {
                        Some(Choice::Literal(wrapped))
                    }
                    _ => None,
                }
            }
            ResolvedBody::Enum { variants } => (!variants.is_empty()).then(|| {
                Choice::Enum(
                    variants
                        .iter()
                        .map(|variant| variant.name().to_owned())
                        .collect(),
                )
            }),
            ResolvedBody::Struct { .. } => None,
        },
        ResolvedTypeRef::Map { key, value } => {
            let key = minted(ir, &ResolvedTypeRef::Primitive { name: *key }, true)
                .filter(|key| matches!(key, Minted::Uuid | Minted::Text { .. }))?;
            Some(Choice::Entry {
                key,
                value: Box::new(choose(ir, value, depth + 1)?),
            })
        }
        ResolvedTypeRef::Optional { .. }
        | ResolvedTypeRef::List { .. }
        | ResolvedTypeRef::Primitive { .. } => None,
    }
}

/// Whether `literal` is a value of `kind`, a chain of newtypes over a primitive: the primitive
/// admits it ([`Primitive::admits`]), and at every layer its text starts with the prefix, draws
/// from the alphabet, and every invariant is true with it bound at `value`, as ESS evaluates one.
pub fn admits(ir: &EssIr, kind: &ResolvedTypeRef, literal: &FactValue) -> bool {
    let mut kind = kind;
    for _ in 0..=MAX_TYPE_DEPTH {
        match kind {
            ResolvedTypeRef::Primitive { name } => {
                let node = match literal {
                    FactValue::Bool(value) => Node::Bool(*value),
                    FactValue::Number(number) => Node::Number(*number),
                    FactValue::Text(text) => Node::Text(text.clone()),
                };
                return name.admits(&node).is_some();
            }
            ResolvedTypeRef::Declared { name } => {
                let ResolvedBody::Newtype {
                    of,
                    alphabet,
                    prefix,
                    invariants,
                } = &ir.named_type(name).body
                else {
                    return false;
                };
                if let FactValue::Text(text) = literal
                    && (prefix
                        .as_ref()
                        .is_some_and(|prefix| !text.starts_with(prefix.as_str()))
                        || alphabet.as_ref().is_some_and(|alphabet| {
                            text.chars().any(|character| !alphabet.contains(character))
                        }))
                {
                    return false;
                }
                let mut facts = FactStore::new();
                facts.set_path("value", literal.clone());
                if !invariants
                    .iter()
                    .all(|invariant| invariant.predicate.evaluate(&facts) == Truth::True)
                {
                    return false;
                }
                kind = of;
            }
            _ => return false,
        }
    }
    false
}

/// The most literals one newtype offers.
const MAX_LITERALS: usize = 512;

/// The literals `predicate` compares with, in order, each number followed by its neighbours one
/// above and one below: the candidates ESS's interpreter tries (`interpret::response::literals`).
fn literals(predicate: &Predicate, values: &mut Vec<FactValue>) {
    match predicate {
        Predicate::All(children) | Predicate::Any(children) => {
            for child in children {
                literals(child, values);
            }
        }
        Predicate::Not(child) => literals(child, values),
        Predicate::Compare { left, right, .. } => {
            for operand in [left, right] {
                let Operand::Literal(value) = operand else {
                    continue;
                };
                let found = match value {
                    FactValue::Number(number) => [
                        Some(*number),
                        number.checked_add(1i64.into()),
                        number.checked_add((-1i64).into()),
                    ]
                    .into_iter()
                    .flatten()
                    .map(FactValue::Number)
                    .collect(),
                    other => vec![other.clone()],
                };
                for value in found {
                    if values.len() < MAX_LITERALS {
                        values.push(value);
                    }
                }
            }
        }
        _ => {}
    }
}
