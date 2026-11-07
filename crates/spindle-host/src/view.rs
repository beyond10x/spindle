//! Answering a view from the store of the component that owns its source: one row by identity,
//! or every row the view holds, kept by its filter and ranked by its `order_by`.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBody, ResolvedEntity, ResolvedField, ResolvedTypeRef, ResolvedView,
};
use ess_conformance::input::Completeness;
use ess_domain::expression::{Resolution, ScalarKind};
use ess_domain::types::{MAX_TYPE_DEPTH, Presence, Primitive};
use ess_domain::view::{Direction, ViewSpec};
use ess_primitives::facts::FactSource;
use ess_primitives::node::Node;
use ess_primitives::predicate::{Collection, CompareOp, Operand, Predicate, Truth};
use ess_primitives::time::CurrentTime;
use ess_primitives::{FactPath, FactStore, FactValue, Rfc3339Instant};
use serde_json::{Map, Value};
use spindle_kernel::Row;

/// The one fact path a filter read here may name: the lifecycle state.
const STATE: &str = "state";

/// Why a view cannot be answered here: the constructs a read of the source's own rows does not
/// cover, named.
///
/// A filter is read only where each leaf, as the compiler parsed it, is one of
/// (`story:view-list-query-parameters`, `story:view-field-literal-filters`):
///
/// - an observable field of the source (the state and the identity among them) compared with a
///   literal of its type by `==` or `!=`, or by `<`, `<=`, `>` or `>=` where ESS orders the type
///   (a number, or text other than a `Duration`): `archived == false`, `priority >= 3`,
///   `state == Open`; a `Timestamp` field ordered against `now`, `now - 1h` or `now + 5m`
///   (`issued_at < now`), the current-time operand ESS reads against the clock of the read
///   ([`reads_now`]);
/// - such a field listed `in:` or `none_of:` literals of its type;
/// - a parameter compared with `==` or `!=` to such a field (`book_id == param.id`);
/// - `defined(param.<name>)` or `defined(<field>)`;
///
/// leaves combined by `any`, `all` and `not`, or under `exists` or `forall` over a list parameter
/// or a list field of the row. A list parameter's element is read as a parameter is; a list
/// field's element as a field is, and is also compared with `==` or `!=` to an outer binder. Any
/// other filter is named with its text and refused whole, since evaluating part of it would answer
/// a subset of its rows.
pub(crate) fn unsupported(ir: &EssIr, view: &ResolvedView) -> Option<String> {
    if !view.computed.is_empty() {
        Some("computed fields".to_owned())
    } else if view.aggregation.is_some() {
        Some("an aggregation".to_owned())
    } else if !view.references.is_empty() {
        Some("fields read through a reference".to_owned())
    } else if view.absent.is_some() {
        Some("a default row for an absent identity".to_owned())
    } else if view.paging.is_some() {
        Some("paging".to_owned())
    } else {
        match &view.filter {
            Some(filter) if !reads(ir, view, filter) => Some(format!(
                "the filter `{filter}`; only a field compared with a literal of its type or \
                 ordered against `now`, or listed `in:` or `none_of:` literals, a parameter compared with a field by `==` \
                 or `!=`, and `defined`, combined by `any`, `all` and `not` or quantified over a \
                 list parameter or a list field by `exists` or `forall`, are read"
            )),
            _ => None,
        }
    }
}

/// Whether every leaf of `filter` is one [`unsupported`] admits. Each path is typed as ESS types
/// it ([`ess_compiler::expression::resolve_path`]): a field against the source's observable
/// fields, a parameter against the view's, and a binder as the element (`<list>.0`) of the list it
/// walks. ESS evaluates a quantifier over the elements a list publishes (`<name>.count`,
/// `<name>.<index>`). The parser bounds the nesting at `MAX_PREDICATE_DEPTH`.
fn reads(ir: &EssIr, view: &ResolvedView, filter: &Predicate) -> bool {
    let fields = ir.entity(&view.source).observable_fields();
    let scope = Scope {
        ir,
        fields: &fields,
        params: &view.params,
        binders: Vec::new(),
    };
    scope.admits(filter)
}

/// A quantifier's binder: its name, the path of the element it reads (`tags.0`,
/// `param.wanted.0`), and whether that element is of the row (a list field) or of the read (a
/// list parameter).
#[derive(Clone)]
struct Binder<'p> {
    name: &'p str,
    element: FactPath,
    row: bool,
}

/// What a leaf of a filter may name where it stands: the source's fields, the view's parameters
/// and the binders of the quantifiers it is inside, innermost last.
struct Scope<'a> {
    ir: &'a EssIr,
    fields: &'a [ResolvedField],
    params: &'a [ResolvedField],
    binders: Vec<Binder<'a>>,
}

/// What a path a leaf reads is: of the row (a field, or an element of a list field) or of the
/// read (a parameter, or an element of a list parameter), a binder or not, and its type as ESS
/// resolves it.
struct Typed {
    row: bool,
    binder: bool,
    resolution: Resolution<ResolvedTypeRef>,
}

impl<'a> Scope<'a> {
    /// The type of `path` resolved over `fields`, as ESS resolves a fact path.
    fn resolve(
        &self,
        fields: &[ResolvedField],
        path: &FactPath,
    ) -> Option<Resolution<ResolvedTypeRef>> {
        ess_compiler::expression::resolve_path(self.ir, fields, path, "a view filter").ok()
    }

    /// What `path` names here, or `None` where it is no field of the source, no declared
    /// parameter and no binder in scope.
    fn typed(&self, path: &FactPath) -> Option<Typed> {
        match path.segments() {
            [name] => {
                if let Some(binder) = self.binders.iter().rev().find(|binder| binder.name == name) {
                    let (fields, element) = self.source(&binder.element);
                    return Some(Typed {
                        row: binder.row,
                        binder: true,
                        resolution: self.resolve(fields, &element)?,
                    });
                }
                Some(Typed {
                    row: true,
                    binder: false,
                    resolution: self.resolve(self.fields, path)?,
                })
            }
            [namespace, _] if namespace == ViewSpec::PARAM => {
                let (fields, path) = self.source(path);
                Some(Typed {
                    row: false,
                    binder: false,
                    resolution: self.resolve(fields, &path)?,
                })
            }
            _ => None,
        }
    }

    /// The fields `path` is declared among and its path there: a parameter's without `param.`.
    fn source<'p>(
        &self,
        path: &'p FactPath,
    ) -> (&'a [ResolvedField], std::borrow::Cow<'p, FactPath>) {
        parameter(path).map_or((self.fields, std::borrow::Cow::Borrowed(path)), |inner| {
            (self.params, std::borrow::Cow::Owned(inner))
        })
    }

    fn admits(&self, filter: &Predicate) -> bool {
        match filter {
            Predicate::Compare {
                left: Operand::Fact(left),
                op: CompareOp::Eq | CompareOp::Ne,
                right: Operand::Fact(right),
                ..
            } => match (self.typed(left), self.typed(right)) {
                (Some(left), Some(right)) => {
                    left.row != right.row
                        || (left.row && left.binder && right.binder)
                        || (right.row && right.binder && left.binder)
                }
                _ => false,
            },
            Predicate::Compare {
                left: Operand::Fact(path),
                op,
                right: Operand::Literal(literal),
                ..
            }
            | Predicate::Compare {
                left: Operand::Literal(literal),
                op,
                right: Operand::Fact(path),
                ..
            } => self.typed(path).is_some_and(|typed| {
                typed.row
                    && fits(&typed.resolution, literal)
                    && (matches!(op, CompareOp::Eq | CompareOp::Ne) || ordered(&typed.resolution))
            }),
            Predicate::AnyOf { path, values } | Predicate::NoneOf { path, values } => {
                self.typed(path).is_some_and(|typed| {
                    typed.row && values.iter().all(|value| fits(&typed.resolution, value))
                })
            }
            Predicate::Defined(path) => self.typed(path).is_some_and(|typed| !typed.binder),
            Predicate::Exists(quantified) | Predicate::Forall(quantified) => {
                let Collection::Fact(over) = &quantified.over else {
                    return false;
                };
                let Some(typed) = self.typed(over) else {
                    return false;
                };
                if typed.binder {
                    return false;
                }
                let element = FactPath::from_segments(
                    over.segments()
                        .iter()
                        .cloned()
                        .chain(std::iter::once("0".to_owned()))
                        .collect::<Vec<_>>(),
                );
                let (fields, at) = self.source(&element);
                if !matches!(typed.resolution.terminal, ResolvedTypeRef::List { .. })
                    || self.resolve(fields, &at).is_none()
                {
                    return false;
                }
                let mut binders = self.binders.clone();
                binders.push(Binder {
                    name: quantified.bind.as_str(),
                    element,
                    row: typed.row,
                });
                Scope {
                    ir: self.ir,
                    fields: self.fields,
                    params: self.params,
                    binders,
                }
                .admits(&quantified.body)
            }
            Predicate::Any(children) | Predicate::All(children) => {
                children.iter().all(|child| self.admits(child))
            }
            Predicate::Not(child) => self.admits(child),
            _ => false,
        }
    }
}

/// `path` without its `param.` namespace, where it reads a parameter.
fn parameter(path: &FactPath) -> Option<FactPath> {
    match path.segments() {
        [namespace, rest @ ..] if namespace == ViewSpec::PARAM && !rest.is_empty() => {
            Some(FactPath::from_segments(rest))
        }
        _ => None,
    }
}

/// Whether `literal` is a value of the scalar `resolution` names, as ESS's checker compares the
/// two: a Boolean, a number, or text (an enum's and every text-backed primitive's).
fn fits(resolution: &Resolution<ResolvedTypeRef>, literal: &FactValue) -> bool {
    matches!(
        (resolution.scalar, literal),
        (Some(ScalarKind::Bool), FactValue::Bool(_))
            | (Some(ScalarKind::Number), FactValue::Number(_))
            | (Some(ScalarKind::Text), FactValue::Text(_))
    )
}

/// Whether ESS orders values of the scalar `resolution` names: a number, or text that is no
/// `Duration` (`FactSource::orders_text_by_bytes`, a `Timestamp` by its instant).
fn ordered(resolution: &Resolution<ResolvedTypeRef>) -> bool {
    match resolution.scalar {
        Some(ScalarKind::Number) => true,
        Some(ScalarKind::Text) => !matches!(
            resolution.terminal,
            ResolvedTypeRef::Primitive {
                name: Primitive::Duration
            }
        ),
        _ => false,
    }
}

/// The values `given` holds for `view`'s parameters, by declared name, as the facts its filter
/// reads under `param.`. Each value given is checked as ESS's interpreter checks a view parameter
/// (`interpret/views.rs:45` `query`, `input::validate_typed_value`) by [`spindle_kernel::admits`],
/// the check every command input goes through: its type, and a newtype's prefix, alphabet and
/// invariants. It is then bound by ESS ([`ess_conformance::input::bind`]).
///
/// # Errors
/// Naming the parameter: one `view` does not declare, a required one `given` leaves out (or holds
/// as `null`), or one whose value is not of its declared type or breaks its newtype.
pub(crate) fn parameters(
    ir: &EssIr,
    view: &ResolvedView,
    given: &Map<String, Value>,
) -> Result<FactStore, String> {
    if let Some(name) = given
        .keys()
        .find(|name| !view.params.iter().any(|param| &param.name == *name))
    {
        return Err(format!("`{name}` is not a parameter of {}", view.name));
    }
    let mut values = BTreeMap::new();
    for param in &view.params {
        match given.get(&param.name).filter(|value| !value.is_null()) {
            Some(value) => {
                spindle_kernel::admits(ir, &param.type_ref, value)
                    .map_err(|error| format!("`{}`: {error}", param.name))?;
                let node = serde_json::from_value::<Node>(value.clone())
                    .map_err(|error| format!("`{}`: {error}", param.name))?;
                values.insert(param.name.clone(), node);
            }
            None if param.type_ref.is_optional() => {}
            None => return Err(format!("the parameter `{}` is required", param.name)),
        }
    }
    ess_conformance::input::bind(ir, &view.params, &values, Completeness::Partial)
        .map_err(|errors| format!("the parameter {errors}"))
}

/// What a filter reads: the row's facts at their own paths, typed by the source's observable
/// fields, and the parameters under `param.`, typed by the view's, as ESS's interpreter reads a
/// view's filter (`ess-conformance` `src/interpret/views.rs`, `ViewFacts` over `TypedFacts`): a
/// `Timestamp` is ordered by the instant it names, any text but a `Duration` by its bytes, and a
/// `now` operand against `now`, the instant of the read (`ViewFacts::now`).
struct Read<'a> {
    ir: &'a EssIr,
    fields: &'a [ResolvedField],
    params: &'a [ResolvedField],
    row: &'a FactStore,
    parameters: &'a FactStore,
    now: Option<Rfc3339Instant>,
}

impl Read<'_> {
    fn source<'p>(
        &self,
        path: &'p FactPath,
    ) -> (&FactStore, &[ResolvedField], std::borrow::Cow<'p, FactPath>) {
        parameter(path).map_or(
            (self.row, self.fields, std::borrow::Cow::Borrowed(path)),
            |inner| (self.parameters, self.params, std::borrow::Cow::Owned(inner)),
        )
    }

    /// The primitive `path` is declared as, through every newtype and `Optional`, and through a
    /// list to its element at an ordinal; `None` for anything else, `state` included.
    fn declared(&self, path: &FactPath) -> Option<Primitive> {
        let (_, fields, path) = self.source(path);
        match ess_compiler::expression::resolve_path(self.ir, fields, &path, "a view filter")
            .ok()?
            .terminal
        {
            ResolvedTypeRef::Primitive { name } => Some(name),
            _ => None,
        }
    }
}

impl FactSource for Read<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        let (source, _, path) = self.source(path);
        source.fact(&path)
    }

    fn present(&self, path: &FactPath) -> bool {
        let (source, _, path) = self.source(path);
        source.present(&path)
    }

    fn scales(&self) -> &ess_primitives::facts::Scales {
        self.row.scales()
    }

    fn orders_as_instant(&self, path: &FactPath) -> bool {
        self.declared(path) == Some(Primitive::Timestamp)
    }

    fn orders_text_by_bytes(&self, path: &FactPath) -> bool {
        self.declared(path) != Some(Primitive::Duration)
    }

    fn now(&self) -> Option<Rfc3339Instant> {
        self.now
    }
}

/// Whether `filter` orders a fact against the current time: a comparison by `<`, `<=`, `>` or
/// `>=` of a fact with a literal that is ESS's current-time operand (`now`, `now - 60s`,
/// [`CurrentTime`]), under any `any`, `all`, `not`, `exists` or `forall`. Where it does, a read
/// takes the clock's instant once, before its first row, as ESS's interpreter reads its clock for
/// such a filter and for no other (`Interpreted::query_view`, `now_offset::now_compared` at ESS
/// 0.89.0), so every row of one read is held against one instant.
pub(crate) fn reads_now(filter: &Predicate) -> bool {
    match filter {
        Predicate::All(children) | Predicate::Any(children) => children.iter().any(reads_now),
        Predicate::Not(inner) => reads_now(inner),
        Predicate::Forall(quantified) | Predicate::Exists(quantified) => {
            reads_now(&quantified.body)
        }
        Predicate::Compare {
            left, op, right, ..
        } if op.needs_ordering() => [(left, right), (right, left)].iter().any(|pair| {
            matches!(
                pair,
                (Operand::Fact(_), Operand::Literal(FactValue::Text(text)))
                    if CurrentTime::parse(text).is_some()
            )
        }),
        _ => false,
    }
}

/// The instant the civil time `at` names, the millisecond it falls in, as ESS turns a clock's
/// reading into one ([`Rfc3339Instant::from_epoch_millis`]); `None` past the years RFC 3339
/// spells.
pub(crate) fn instant(at: SystemTime) -> Option<Rfc3339Instant> {
    let millis = match at.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_millis()).ok()?,
        Err(before) => {
            let before = before.duration();
            let whole = i64::try_from(before.as_millis()).ok()?;
            let partial = i64::from(before.subsec_nanos() % 1_000_000 != 0);
            whole.checked_add(partial)?.checked_neg()?
        }
    };
    Rfc3339Instant::from_epoch_millis(millis)
}

/// Whether the view holds `row`, the instance `identity` names: always without a filter; with
/// one, where ESS evaluates the filter to true over the row and `parameters`
/// ([`parameters`]). Only a filter [`unsupported`] admits is read.
///
/// A view without parameters whose filter reads nothing but the state reads the state alone;
/// any other binds the state, the identity and every stored field, as ESS binds a source row
/// ([`ess_conformance::input::bind`] over the entity's observable fields). A field the row holds
/// no value for is absent, as ESS reads it: a comparison with it is unknown, `defined` false.
/// A `now` operand is read as `now`, the instant of the read the caller took ([`reads_now`]); with
/// none it is unknown, as ESS reads it without a clock.
///
/// A filter unknown for the row is refused, as ESS's interpreter refuses the read
/// (`ess-conformance` `src/interpret/views.rs:214` at ESS 0.89.0, `select`,
/// `Truth::Unknown => return Err(unsupported())`): one comparing against an `Optional` parameter
/// the read left out, or an `Optional` field the row does not hold, is unknown, not false. The
/// interpreter hides such a row only where the omitted parameter is read by a string operator
/// (`absent_text_operands`, `views.rs:210`), or where a path read through a reference reads no
/// row (`absent_navigation`), and no filter [`unsupported`] admits has either.
///
/// # Errors
/// When the row is not a value of the source's declared fields, naming what it holds, or when the
/// filter is unknown for it.
pub(crate) fn holds(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
    parameters: &FactStore,
    now: Option<Rfc3339Instant>,
) -> Result<bool, String> {
    let Some(filter) = &view.filter else {
        return Ok(true);
    };
    let entity = ir.entity(&view.source);
    let fields = entity.observable_fields();
    let state_alone = view.params.is_empty()
        && filter
            .fact_paths()
            .iter()
            .all(|path| path.segments() == [STATE]);
    let facts = if state_alone {
        let mut facts = FactStore::new();
        if let Ok(path) = FactPath::new(STATE) {
            facts.set(path, FactValue::text(row.state.clone()));
        }
        facts
    } else {
        let mut held = row
            .fields
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        held.insert(entity.identity.name.clone(), identity.clone());
        held.insert(STATE.to_owned(), Value::String(row.state.clone()));
        let held: BTreeMap<String, Node> = serde_json::from_value(Value::Object(held))
            .map_err(|error| format!("a row of {}: {error}", entity.name))?;
        ess_conformance::input::bind(ir, &fields, &held, Completeness::Partial)
            .map_err(|errors| format!("a row of {}: {errors}", entity.name))?
    };
    let read = Read {
        ir,
        fields: &fields,
        params: &view.params,
        row: &facts,
        parameters,
        now,
    };
    match filter.evaluate(&read) {
        Truth::True => Ok(true),
        Truth::False => Ok(false),
        Truth::Unknown => Err(format!(
            "the filter `{filter}` is unknown for a row, so the model does not determine the rows \
             it holds"
        )),
    }
}

/// The view's row for the instance `identity` names, held as `row`.
///
/// A field typed as the source's lifecycle state answers the state the row holds; the field named
/// as the source's identity answers `identity`; every other field answers the stored field of that
/// name. Where the row holds nothing for it, the field follows the presence the compiler carried
/// onto it from its source field: `null` under `null_when_absent`, left out otherwise.
pub(crate) fn project(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
) -> Map<String, Value> {
    let entity: &ResolvedEntity = ir.entity(&view.source);
    let mut answer = Map::new();
    for field in &view.fields {
        let value = if field.type_ref.required().declared() == Some(&entity.state_type) {
            Some(Value::String(row.state.clone()))
        } else if field.name == entity.identity.name {
            Some(identity.clone())
        } else {
            row.fields
                .get(&field.name)
                .filter(|value| !value.is_null())
                .cloned()
        };
        match value {
            Some(value) => {
                answer.insert(field.name.clone(), value);
            }
            None if field.naming.presence == Some(Presence::NullWhenAbsent) => {
                answer.insert(field.name.clone(), Value::Null);
            }
            None => {}
        }
    }
    answer
}

/// Sorts `rows`, each an identity and the view's row for it, into the order the view answers:
/// by each `order_by` key in turn, descending where the key says so, then by identity ascending,
/// so rows tied on every key, and every row of a view without `order_by`, come in one order on
/// every store.
///
/// A key ranks by its field's declared type, through `Optional` and newtypes: a `Timestamp` by the
/// instant it names, not its spelling, and one that names no instant below every one that does;
/// a number by its exact value; anything else by its JSON value,
/// text by its UTF-8 bytes. A field a row does not carry ranks below every value.
pub(crate) fn order(ir: &EssIr, view: &ResolvedView, rows: &mut [(Value, Map<String, Value>)]) {
    let entity = ir.entity(&view.source);
    let identity = primitive(ir, &entity.identity.type_ref);
    let keys = view
        .order_by
        .iter()
        .map(|ranking| {
            let kind = view
                .fields
                .iter()
                .find(|field| field.name == ranking.field)
                .and_then(|field| primitive(ir, &field.type_ref));
            (ranking.field.as_str(), ranking.direction, kind)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|(left_id, left), (right_id, right)| {
        for (field, direction, kind) in &keys {
            let ordering = rank(*kind, left.get(*field), right.get(*field));
            let ordering = match direction {
                Direction::Ascending => ordering,
                Direction::Descending => ordering.reverse(),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        rank(identity, Some(left_id), Some(right_id))
    });
}

/// The primitive `type_ref` is underneath `Optional` and every newtype; `None` for an enum, a
/// struct, a list or anything else that is not a primitive.
fn primitive(ir: &EssIr, type_ref: &ResolvedTypeRef) -> Option<Primitive> {
    let mut current = type_ref.required();
    for _ in 0..=MAX_TYPE_DEPTH {
        match current {
            ResolvedTypeRef::Primitive { name } => return Some(*name),
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype { of, .. } => current = of.required(),
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}

/// How two values of one key rank; an absent value, or `null`, below every value.
fn rank(kind: Option<Primitive>, left: Option<&Value>, right: Option<&Value>) -> Ordering {
    let left = left.filter(|value| !value.is_null());
    let right = right.filter(|value| !value.is_null());
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => value(kind, left, right),
    }
}

/// How two present values of one key rank. Every rule is a total order, as a sort needs: one
/// rule that is not lets a third row reorder two others.
///
/// - `Timestamp`: a value that parses as an RFC 3339 instant ranks by that instant; one that does
///   not (ESS admits any text as a `Timestamp`) ranks below every one that does, and two such
///   values rank by their JSON value as below. Two spellings of one instant tie.
/// - Numbers rank by their exact value, integer against float included, never through a lossy
///   conversion.
/// - Text ranks by its UTF-8 bytes, booleans `false` before `true`, and any other pair by the
///   bytes of its JSON rendering.
fn value(kind: Option<Primitive>, left: &Value, right: &Value) -> Ordering {
    if kind == Some(Primitive::Timestamp) {
        let instant = |value: &Value| value.as_str().and_then(Rfc3339Instant::parse_rfc3339);
        match (instant(left), instant(right)) {
            (Some(left), Some(right)) => return left.cmp(&right),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => {}
        }
    }
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => number(left, right),
        (Value::String(left), Value::String(right)) => left.as_bytes().cmp(right.as_bytes()),
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (left, right) => left.to_string().cmp(&right.to_string()),
    }
}

/// Two JSON numbers by exact value. Integers compare as `i128`, which holds every `i64` and
/// `u64`; an integer against a float compares through the float only where the rounded integer
/// differs from it, since rounding keeps order; where they are equal the float is integral and
/// compares exactly as an integer.
fn number(left: &serde_json::Number, right: &serde_json::Number) -> Ordering {
    let integer = |number: &serde_json::Number| {
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
    };
    let against = |integer: i128, float: Option<f64>| {
        float.map_or(Ordering::Equal, |float| {
            match (integer as f64).partial_cmp(&float) {
                Some(Ordering::Equal) => integer.cmp(&(float as i128)),
                ordering => ordering.unwrap_or(Ordering::Equal),
            }
        })
    };
    match (integer(left), integer(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(left), None) => against(left, right.as_f64()),
        (None, Some(right)) => against(right, left.as_f64()).reverse(),
        (None, None) => left
            .as_f64()
            .zip(right.as_f64())
            .and_then(|(left, right)| left.partial_cmp(&right))
            .unwrap_or(Ordering::Equal),
    }
}
