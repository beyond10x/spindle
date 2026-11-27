//! Whether the rows a durable store holds fit a specification other than the one it was last
//! written under (`story:spec-change-over-stored-rows`).
//!
//! A store records the specification it was written under ([`Shapes`]): the model's digest
//! (`SuiteProvenance::spec_digest`) and the shape of each entity it keeps, every field's type by
//! structure and every lifecycle state. A run on the store under another specification compares
//! the recorded shapes with the new ones, for the entities that hold rows only, and refuses where
//! the new specification cannot read a stored row ([`Misfit`]): a field removed while a row holds
//! it, a field retyped while a row holds it (types compared by structure, so a named type
//! redefined under the same name counts; a field that becomes optional keeps its rows), a
//! non-optional field added, or made required, while rows lack it, a lifecycle state removed
//! while a row rests in it, or the entity removed. Nothing is migrated. This is Spindle's own rule over the recorded
//! shapes, not ESS's: `ess-diff` 0.99.2 answers `unknown` for every entity field change.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedTypeRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::store::{Store, StoreError};

/// The specification a store was written under: the model's digest and the shape of each entity
/// the store keeps, by qualified name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shapes {
    /// The digest of the resolved model, as a conformance suite's provenance names it.
    pub digest: String,
    /// Each entity's shape, by qualified name.
    pub entities: BTreeMap<String, Shape>,
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
            let held = Held::of(store, entity)?;
            if held.rows == 0 {
                continue;
            }
            let Some(shape) = self.entities.get(entity) else {
                misfits.push(Misfit::Entity {
                    entity: entity.clone(),
                    rows: held.rows,
                });
                continue;
            };
            let Some(was) = recorded.entities.get(entity) else {
                continue;
            };
            for (field, kind) in &was.fields {
                let holding = held.fields.get(field).copied().unwrap_or(0);
                if holding == 0 {
                    continue;
                }
                match shape.fields.get(field) {
                    None => misfits.push(Misfit::Removed {
                        entity: entity.clone(),
                        field: field.clone(),
                        rows: holding,
                    }),
                    Some(now) if !reads(now, kind) => misfits.push(Misfit::Retyped {
                        entity: entity.clone(),
                        field: field.clone(),
                        rows: holding,
                    }),
                    Some(_) => {}
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
}

impl Misfit {
    /// The entity it names.
    #[must_use]
    pub fn entity(&self) -> &str {
        match self {
            Self::Entity { entity, .. }
            | Self::Removed { entity, .. }
            | Self::Retyped { entity, .. }
            | Self::Required { entity, .. }
            | Self::State { entity, .. } => entity,
        }
    }

    /// The field it names, if it names one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Removed { field, .. }
            | Self::Retyped { field, .. }
            | Self::Required { field, .. } => Some(field),
            Self::Entity { .. } | Self::State { .. } => None,
        }
    }
}

impl fmt::Display for Misfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entity { entity, rows } => {
                write!(
                    f,
                    "{entity}: the entity is removed; stored rows of it: {rows}"
                )
            }
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
        }
    }
}

/// What the rows of one entity hold: how many rest, how many hold each field (a value, not
/// `null`), and how many rest in each state.
#[derive(Default)]
struct Held {
    rows: usize,
    fields: BTreeMap<String, usize>,
    states: BTreeMap<String, usize>,
}

impl Held {
    fn of(store: &mut dyn Store, entity: &str) -> Result<Self, StoreError> {
        let mut held = Self::default();
        for (_, stored) in store.scan(entity)? {
            held.rows += 1;
            *held.states.entry(stored.row.state).or_default() += 1;
            for (field, value) in stored.row.fields {
                if !value.is_null() {
                    *held.fields.entry(field).or_default() += 1;
                }
            }
        }
        Ok(held)
    }
}

/// Whether a field of type `now` reads a value stored as `was`: the same structure, or `was` made
/// optional.
fn reads(now: &Value, was: &Value) -> bool {
    now == was || now.get("optional").is_some_and(|of| of == was)
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
                ResolvedBody::Newtype { of, .. } => json!({ "newtype": structure(ir, of, within) }),
                ResolvedBody::Struct { fields, .. } => {
                    let fields: serde_json::Map<String, Value> = fields
                        .iter()
                        .map(|field| (field.name.clone(), structure(ir, &field.type_ref, within)))
                        .collect();
                    json!({ "struct": fields })
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
