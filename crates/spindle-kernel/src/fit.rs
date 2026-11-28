//! Whether the rows a durable store holds fit a specification other than the one it was last
//! written under (`story:spec-change-over-stored-rows`).
//!
//! A store records the specification it was written under ([`Shapes`]): the model's digest
//! (`SuiteProvenance::spec_digest`) and the shape of each entity it keeps, every field's type by
//! structure and every lifecycle state. A run on the store under another specification compares
//! the recorded shapes with the new ones, for the entities that hold rows only, and refuses where
//! the new specification cannot read a stored row ([`Misfit`]): a field removed while a row holds
//! a value of it, a field retyped while a row holds a value the new type does not read, a
//! non-optional field added, or made required, while rows lack it, a lifecycle state removed
//! while a row rests in it, the entity removed or moved to another component's store, or an
//! invariant the new model declares on the entity that a stored row breaks. Nothing is migrated.
//!
//! Each invariant the new model declares on an entity is judged over every stored row of it, as
//! a row at rest is judged ([`crate::at_rest`]): a row that leaves it `Unknown` holds it
//! (`story:entity-invariant-over-stored-rows`). The record keeps no invariant, so each is judged
//! whenever the digest differs, also one the recorded specification declared.
//!
//! A change fits when every stored value reads under the new shape, judged by value where the
//! shape alone would refuse a readable row (coordinator decision D). Types are compared by
//! structure first: a named type is written out as its body, with a newtype's alphabet, prefix
//! and invariants and a struct's invariants and whether it ignores undeclared members, so one
//! redefined under the same name differs. The
//! same structure, the field made optional, or made required (the rows lacking it counted apart)
//! reads every row. Any other change reads each stored value under the new type as the kernel
//! admits a command's value ([`crate::typed`]: an enum that gains a value, a struct that gains an
//! optional member, a newtype whose prefix, alphabet or invariants every stored value meets),
//! and misfits where one does not. Shapes read back from a record, or built without a model,
//! judge by structure alone. This is Spindle's own rule over the recorded shapes, not ESS's:
//! `ess-diff` 0.99.2 answers `unknown` for every entity field change.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedEntity, ResolvedTypeRef};
use ess_domain::entity::Invariant;
use ess_primitives::predicate::Truth;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::facts::Facts;
use crate::store::{Store, StoreError};

/// The specification a store was written under: the model's digest and the shape of each entity
/// the store keeps, by qualified name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shapes {
    /// The digest of the resolved model, as a conformance suite's provenance names it.
    pub digest: String,
    /// Each entity's shape, by qualified name.
    pub entities: BTreeMap<String, Shape>,
    /// The model the shapes were taken from, to read stored values by and to find the component
    /// an entity moved to: not recorded and not compared.
    #[serde(skip)]
    pub model: Model,
}

/// The model a [`Shapes`] was taken from ([`Shapes::of`], [`Shapes::owned`]); none for shapes read
/// back from a record or built by hand. Every two are equal: the model is no part of what a store
/// records.
#[derive(Clone, Default)]
pub struct Model(Option<Arc<EssIr>>);

impl PartialEq for Model {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Model {}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "Model(..)"
        } else {
            "Model(none)"
        })
    }
}

/// One entity's shape as a store records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shape {
    /// Each field's type by structure, by field name: a named type is written out as its body,
    /// so one redefined under the same name differs.
    pub fields: BTreeMap<String, Value>,
    /// Its lifecycle states.
    pub states: BTreeSet<String>,
}

impl Shapes {
    /// The shapes of every entity of `ir`: what a store that keeps every component's rows (the
    /// one SQLite file of `run` reading stdin) records.
    #[must_use]
    pub fn of(ir: &EssIr) -> Self {
        Self::kept(ir, |_| true)
    }

    /// The shapes of the entities whose domain the component `component` owns: what that
    /// component's own store records.
    #[must_use]
    pub fn owned(ir: &EssIr, component: &str) -> Self {
        let owns = ir
            .components()
            .values()
            .find(|candidate| candidate.name.as_str() == component)
            .map(|component| component.owns.clone())
            .unwrap_or_default();
        Self::kept(ir, |entity| owns.contains(&entity.domain))
    }

    fn kept(ir: &EssIr, keeps: impl Fn(&ess_compiler::ir::ResolvedEntity) -> bool) -> Self {
        let entities = ir
            .entities()
            .values()
            .filter(|entity| keeps(entity))
            .map(|entity| {
                let fields = entity
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.clone(),
                            structure(ir, &field.type_ref, &mut Vec::new()),
                        )
                    })
                    .collect();
                let states = entity
                    .lifecycle
                    .states
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                (entity.name.to_string(), Shape { fields, states })
            })
            .collect();
        Self {
            digest: ess_conformance::SuiteProvenance::of(ir)
                .spec_digest
                .to_string(),
            entities,
            model: Model(Some(Arc::new(ir.clone()))),
        }
    }

    /// Every way the rows `store` holds do not fit `self`, a store that recorded `recorded` last:
    /// empty where every stored row stays readable. Reads the rows of each entity either names.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    pub fn misfits(
        &self,
        recorded: &Self,
        store: &mut dyn Store,
    ) -> Result<Vec<Misfit>, StoreError> {
        let mut misfits = Vec::new();
        let named: BTreeSet<&String> = recorded
            .entities
            .keys()
            .chain(self.entities.keys())
            .collect();
        for entity in named {
            let shape = self.entities.get(entity);
            let was = recorded.entities.get(entity);
            // The fields whose type changed so that the structure alone does not read every
            // value: each stored value of them is read under the new type, where the model is
            // at hand, and otherwise each row holding one misfits.
            let reread: BTreeMap<&str, Option<&ResolvedTypeRef>> = match (shape, was) {
                (Some(shape), Some(was)) => was
                    .fields
                    .iter()
                    .filter_map(|(field, kind)| {
                        let now = shape.fields.get(field)?;
                        (!reads(now, kind))
                            .then(|| (field.as_str(), self.model.field(entity, field)))
                    })
                    .collect(),
                _ => BTreeMap::new(),
            };
            // The entity as the new model declares it, every stored row judged by its invariants.
            let judged = shape.and(self.model.ir()).zip(self.model.entity(entity));
            let held = Held::of(store, entity, self.model.ir(), &reread, judged)?;
            if held.rows == 0 {
                continue;
            }
            let Some(shape) = shape else {
                misfits.push(match self.model.owner(entity) {
                    Some(component) => Misfit::Moved {
                        entity: entity.clone(),
                        component,
                        rows: held.rows,
                    },
                    None => Misfit::Entity {
                        entity: entity.clone(),
                        rows: held.rows,
                    },
                });
                continue;
            };
            if let Some((_, declared)) = judged {
                for (invariant, rows) in declared.invariants.iter().zip(&held.broken) {
                    if *rows > 0 {
                        misfits.push(Misfit::Invariant {
                            entity: entity.clone(),
                            invariant: invariant.statement.clone(),
                            rows: *rows,
                        });
                    }
                }
            }
            let Some(was) = was else {
                continue;
            };
            for field in was.fields.keys() {
                let holding = held.fields.get(field).copied().unwrap_or(0);
                if holding == 0 {
                    continue;
                }
                if !shape.fields.contains_key(field) {
                    misfits.push(Misfit::Removed {
                        entity: entity.clone(),
                        field: field.clone(),
                        rows: holding,
                    });
                } else if held.unread.get(field).is_some_and(|unread| *unread > 0) {
                    misfits.push(Misfit::Retyped {
                        entity: entity.clone(),
                        field: field.clone(),
                        rows: holding,
                    });
                }
            }
            for (field, kind) in &shape.fields {
                let required_now = !optional(kind) && was.fields.get(field).is_none_or(optional);
                let lacking = held.rows - held.fields.get(field).copied().unwrap_or(0);
                if required_now && lacking > 0 {
                    misfits.push(Misfit::Required {
                        entity: entity.clone(),
                        field: field.clone(),
                        rows: lacking,
                    });
                }
            }
            for (state, rows) in &held.states {
                if !shape.states.contains(state) {
                    misfits.push(Misfit::State {
                        entity: entity.clone(),
                        state: state.clone(),
                        rows: *rows,
                    });
                }
            }
        }
        Ok(misfits)
    }

    /// Whether `store` holds a row of any entity `self` names.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    pub fn held(&self, store: &mut dyn Store) -> Result<bool, StoreError> {
        let entities: Vec<String> = self.entities.keys().cloned().collect();
        Ok(store.counts(&entities)?.values().any(|count| *count > 0))
    }
}

/// What a store answered when its rows were checked against a specification
/// ([`crate::LogStore::check`], [`crate::LogStore::fit`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fit {
    /// The store was last written under this very specification: no row was read, and nothing
    /// is recorded.
    Same,
    /// The rows fit a specification the store does not record yet: [`crate::LogStore::fit`]
    /// records it. `unrecorded` where the store held rows and recorded no specification, as every
    /// store an earlier release wrote: it fits without a check.
    Fits {
        /// Whether the store held rows and no record.
        unrecorded: bool,
    },
    /// Rows the specification cannot read, each named; nothing is recorded.
    Misfits(Vec<Misfit>),
}

/// One way the stored rows of an entity do not fit a specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Misfit {
    /// The entity is gone while rows of it rest.
    Entity {
        /// The entity.
        entity: String,
        /// How many rows of it rest.
        rows: usize,
    },
    /// The entity's domain is now owned by another component, while rows of it rest in this
    /// store: they are not moved, and the other component's store does not hold them.
    Moved {
        /// The entity.
        entity: String,
        /// The component that owns its domain now.
        component: String,
        /// How many rows of it rest here.
        rows: usize,
    },
    /// A field is gone while rows hold it.
    Removed {
        /// The entity.
        entity: String,
        /// The field.
        field: String,
        /// How many rows hold it.
        rows: usize,
    },
    /// A field's type changed while rows hold it.
    Retyped {
        /// The entity.
        entity: String,
        /// The field.
        field: String,
        /// How many rows hold it.
        rows: usize,
    },
    /// A non-optional field that rows lack, added since or optional before.
    Required {
        /// The entity.
        entity: String,
        /// The field.
        field: String,
        /// How many rows lack it.
        rows: usize,
    },
    /// A lifecycle state is gone while rows rest in it.
    State {
        /// The entity.
        entity: String,
        /// The state.
        state: String,
        /// How many rows rest in it.
        rows: usize,
    },
    /// An invariant the entity declares does not hold of rows that rest, judged as a row at rest
    /// is ([`crate::at_rest`]): a row that leaves it `Unknown` holds it.
    Invariant {
        /// The entity.
        entity: String,
        /// The invariant's statement, as its author wrote it; [`Misfit`]'s `Display` prints it
        /// escaped, as every text a misfit names ([`OneLine`]).
        invariant: String,
        /// How many rows break it.
        rows: usize,
    },
}

impl Misfit {
    /// The entity it names.
    #[must_use]
    pub fn entity(&self) -> &str {
        match self {
            Self::Entity { entity, .. }
            | Self::Moved { entity, .. }
            | Self::Removed { entity, .. }
            | Self::Retyped { entity, .. }
            | Self::Required { entity, .. }
            | Self::State { entity, .. }
            | Self::Invariant { entity, .. } => entity,
        }
    }

    /// The field it names, if it names one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Removed { field, .. }
            | Self::Retyped { field, .. }
            | Self::Required { field, .. } => Some(field),
            Self::Entity { .. }
            | Self::Moved { .. }
            | Self::State { .. }
            | Self::Invariant { .. } => None,
        }
    }
}

/// Writes through to a formatter so that whatever text a misfit names (an invariant's statement,
/// a state read from a stored row) keeps its line one line, holds no control character but a
/// tab, and two texts never print alike: a backslash is written `\\`, a line break `\n`, a
/// carriage return `\r`, every other control character ([`char::is_control`]) and the line and
/// paragraph separators U+2028 and U+2029 as `\u{..}` in hexadecimal; a tab as it is.
struct OneLine<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl fmt::Write for OneLine<'_, '_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut plain = 0;
        for (at, character) in text.char_indices() {
            let named = match character {
                '\\' => Some("\\\\"),
                '\n' => Some("\\n"),
                '\r' => Some("\\r"),
                _ => None,
            };
            let coded = character != '\t'
                && (character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'));
            if named.is_none() && !coded {
                continue;
            }
            self.0.write_str(&text[plain..at])?;
            match named {
                Some(named) => self.0.write_str(named)?,
                None => write!(self.0, "\\u{{{:x}}}", u32::from(character))?,
            }
            plain = at + character.len_utf8();
        }
        self.0.write_str(&text[plain..])
    }
}

/// One line for each misfit, each text it names written apart from any other ([`OneLine`]).
impl fmt::Display for Misfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write as _;
        let f = &mut OneLine(f);
        match self {
            Self::Entity { entity, rows } => {
                write!(
                    f,
                    "{entity}: the entity is removed; stored rows of it: {rows}"
                )
            }
            Self::Moved {
                entity,
                component,
                rows,
            } => write!(
                f,
                "{entity}: the entity moved to component {component}, whose store does not hold \
                 these rows; stored rows of it here: {rows}"
            ),
            Self::Removed {
                entity,
                field,
                rows,
            } => write!(
                f,
                "{entity}.{field}: the field is removed; stored rows holding it: {rows}"
            ),
            Self::Retyped {
                entity,
                field,
                rows,
            } => write!(
                f,
                "{entity}.{field}: the field's type changed; stored rows holding it: {rows}"
            ),
            Self::Required {
                entity,
                field,
                rows,
            } => write!(
                f,
                "{entity}.{field}: the field is required; stored rows lacking it: {rows}"
            ),
            Self::State {
                entity,
                state,
                rows,
            } => write!(
                f,
                "{entity} state {state}: the state is removed; stored rows resting in it: {rows}"
            ),
            Self::Invariant {
                entity,
                invariant,
                rows,
            } => write!(
                f,
                "{entity} invariant `{invariant}`: the invariant does not hold; stored rows \
                 breaking it: {rows}"
            ),
        }
    }
}

impl Model {
    fn ir(&self) -> Option<&EssIr> {
        self.0.as_deref()
    }

    /// The entity `entity` as the model declares it.
    fn entity(&self, entity: &str) -> Option<&ResolvedEntity> {
        self.ir()?
            .entities()
            .values()
            .find(|candidate| candidate.name.to_string() == entity)
    }

    /// The type of `entity`'s field `field` in the model.
    fn field(&self, entity: &str, field: &str) -> Option<&ResolvedTypeRef> {
        self.entity(entity)?
            .fields
            .iter()
            .find(|candidate| candidate.name == field)
            .map(|field| &field.type_ref)
    }

    /// The component that owns `entity`'s domain in the model, where the model declares the
    /// entity.
    fn owner(&self, entity: &str) -> Option<String> {
        let domain = &self.entity(entity)?.domain;
        self.ir()?
            .components()
            .values()
            .find(|component| component.owns.contains(domain))
            .map(|component| component.name.to_string())
    }
}

/// What the rows of one entity hold: how many rest, how many hold each field (a value, not
/// `null`), how many rest in each state, of each field read again, how many hold a value its new
/// type does not read, and of each invariant judged, in the order declared, how many break it.
#[derive(Default)]
struct Held {
    rows: usize,
    fields: BTreeMap<String, usize>,
    states: BTreeMap<String, usize>,
    unread: BTreeMap<String, usize>,
    broken: Vec<usize>,
}

impl Held {
    /// Reads every row of `entity`; each value of a field `reread` names is read under its new
    /// type in `ir`, and is unread where either is missing. Where `judged` gives the entity as a
    /// model declares it, each row is judged by each of its invariants as a row at rest is
    /// (`rests`, `execute.rs`): one the row leaves `Unknown` it holds.
    fn of(
        store: &mut dyn Store,
        entity: &str,
        ir: Option<&EssIr>,
        reread: &BTreeMap<&str, Option<&ResolvedTypeRef>>,
        judged: Option<(&EssIr, &ResolvedEntity)>,
    ) -> Result<Self, StoreError> {
        let mut held = Self {
            broken: vec![0; judged.map_or(0, |(_, declared)| declared.invariants.len())],
            ..Self::default()
        };
        for (identity, stored) in store.scan(entity)? {
            held.rows += 1;
            if let Some((model, declared)) =
                judged.filter(|(_, declared)| !declared.invariants.is_empty())
            {
                let facts = Facts::row(model, declared, &identity, &stored.row, None);
                for (invariant, broken) in declared.invariants.iter().zip(&mut held.broken) {
                    if invariant.predicate.evaluate(&facts) == Truth::False {
                        *broken += 1;
                    }
                }
            }
            *held.states.entry(stored.row.state).or_default() += 1;
            for (field, value) in stored.row.fields {
                if value.is_null() {
                    continue;
                }
                if let Some(now) = reread.get(field.as_str()) {
                    let read = match (ir, now) {
                        (Some(ir), Some(now)) => crate::typed::validate(ir, now, &value).is_ok(),
                        _ => false,
                    };
                    if !read {
                        *held.unread.entry(field.clone()).or_default() += 1;
                    }
                }
                *held.fields.entry(field).or_default() += 1;
            }
        }
        Ok(held)
    }
}

/// Whether a field of type `now` reads every value stored as `was` by structure alone: the same
/// structure, `was` made optional, or `was` made required (the rows lacking a value are
/// [`Misfit::Required`], not a type change).
fn reads(now: &Value, was: &Value) -> bool {
    now == was
        || now.get("optional").is_some_and(|of| of == was)
        || was.get("optional").is_some_and(|of| of == now)
}

fn optional(kind: &Value) -> bool {
    kind.get("optional").is_some()
}

/// `kind` by structure: each named type written out as its body, once on each path (a type met
/// again within itself is written by its name).
fn structure(ir: &EssIr, kind: &ResolvedTypeRef, within: &mut Vec<String>) -> Value {
    match kind {
        ResolvedTypeRef::Primitive { name } => json!({ "primitive": name }),
        ResolvedTypeRef::Optional { of } => json!({ "optional": structure(ir, of, within) }),
        ResolvedTypeRef::List { of } => json!({ "list": structure(ir, of, within) }),
        ResolvedTypeRef::Map { key, value } => {
            json!({ "map": { "key": key, "value": structure(ir, value, within) } })
        }
        ResolvedTypeRef::Declared { name } => {
            let declared = ir.named_type(name);
            let named = declared.name.to_string();
            if within.contains(&named) {
                return json!({ "within": named });
            }
            within.push(named);
            let body = match &declared.body {
                ResolvedBody::Newtype {
                    of,
                    alphabet,
                    prefix,
                    invariants,
                } => {
                    let mut body = serde_json::Map::new();
                    body.insert("newtype".to_owned(), structure(ir, of, within));
                    if let Some(alphabet) = alphabet {
                        body.insert("alphabet".to_owned(), json!(alphabet));
                    }
                    if let Some(prefix) = prefix {
                        body.insert("prefix".to_owned(), json!(prefix));
                    }
                    constrained(&mut body, invariants);
                    Value::Object(body)
                }
                ResolvedBody::Struct { fields, invariants } => {
                    let fields: serde_json::Map<String, Value> = fields
                        .iter()
                        .map(|field| (field.name.clone(), structure(ir, &field.type_ref, within)))
                        .collect();
                    let mut body = serde_json::Map::new();
                    body.insert("struct".to_owned(), Value::Object(fields));
                    if ir.undeclared_fields(&declared.name).is_ignored() {
                        body.insert("undeclared_fields".to_owned(), json!("ignored"));
                    }
                    constrained(&mut body, invariants);
                    Value::Object(body)
                }
                ResolvedBody::Enum { variants } => {
                    let names: BTreeSet<&str> = variants
                        .iter()
                        .map(|variant| variant.name.as_str())
                        .collect();
                    json!({ "enum": names })
                }
                ResolvedBody::Union { tag, variants } => {
                    let variants: serde_json::Map<String, Value> = variants
                        .iter()
                        .map(|(name, payload)| {
                            let payload = payload
                                .as_ref()
                                .map_or(Value::Null, |payload| structure(ir, payload, within));
                            (name.clone(), payload)
                        })
                        .collect();
                    json!({ "union": { "tag": tag, "variants": variants } })
                }
            };
            within.pop();
            body
        }
    }
}

/// Adds `invariants`, as their authors wrote them, to a named type's `body`, where it has any.
fn constrained(body: &mut serde_json::Map<String, Value>, invariants: &[Invariant]) {
    if !invariants.is_empty() {
        let statements: Vec<&str> = invariants
            .iter()
            .map(|invariant| invariant.statement.as_str())
            .collect();
        body.insert("invariants".to_owned(), json!(statements));
    }
}
