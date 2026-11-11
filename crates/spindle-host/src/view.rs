//! Answering a view from the store of the component that owns its source: one row by identity,
//! or every row the view holds, kept by its filter and ranked by its `order_by`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedAggregation, ResolvedBody, ResolvedEntity, ResolvedField, ResolvedTypeRef, ResolvedView,
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
/// A field read through a reference (`field: {related: …}`) is answered ([`project`]), and so is
/// a filter that navigates one (`resource_id.state == Active`,
/// `story:view-filter-through-reference`).
///
/// A filter is read only where each leaf, as the compiler parsed it, is one of
/// (`story:view-list-query-parameters`, `story:view-field-literal-filters`):
///
/// - a field compared with a literal of its type by `==` or `!=`, or by `<`, `<=`, `>` or `>=`
///   where ESS orders the type (a number, or text other than a `Duration`): `archived == false`,
///   `priority >= 3`, `state == Open`; a `Timestamp` field ordered against `now`, `now - 1h` or
///   `now + 5m` (`issued_at < now`), the current-time operand ESS reads against the clock of the
///   read ([`reads_now`]). A field is an observable field of the source (the state and the
///   identity among them), or one of the row a reference names, read through it
///   (`<via>.<field>`, [`ResolvedView::navigated`]);
/// - such a field listed `in:` or `none_of:` literals of its type;
/// - such a field compared with `==` or `!=` to a parameter or another such field
///   (`book_id == param.id`, `class_id.holder == person_id`), or ordered against one where ESS
///   orders both and they are of one scalar kind (`started_at > person_id.joined_at`);
/// - `defined(param.<name>)` or `defined(<field>)`;
///
/// leaves combined by `any`, `all` and `not`, or under `exists` or `forall` over a list parameter
/// or a list field of the row. A list parameter's element is read as a parameter is; a list
/// field's element as a field is, and is also compared with `==` or `!=` to an outer binder. Any
/// other filter is named with its text and refused whole, since evaluating part of it would answer
/// a subset of its rows.
///
/// An aggregation (`group_by:` and fields with `aggregate:`) is answered ([`aggregate`]) where
/// each measure's `where:` is a filter read here, and no field of the view is read through a
/// reference. A computed field (`expr:`) is answered ([`project`]).
pub(crate) fn unsupported(ir: &EssIr, view: &ResolvedView) -> Option<String> {
    if let Some(construct) = view
        .aggregation
        .as_ref()
        .and_then(|aggregation| unread_aggregation(ir, view, aggregation))
    {
        Some(construct)
    } else if view.absent.is_some() {
        Some("a default row for an absent identity".to_owned())
    } else if view.paging.is_some() {
        Some("paging".to_owned())
    } else {
        match &view.filter {
            Some(filter) if !reads(ir, view, filter) => Some(format!(
                "the filter `{filter}`; only a field of the row, or of the row a reference names, \
                 compared with a literal of its type or ordered against `now`, listed `in:` or \
                 `none_of:` literals, or compared with a parameter or another field, and \
                 `defined`, combined by `any`, `all` and `not` or quantified over a list \
                 parameter or a list field by `exists` or `forall`, are read"
            )),
            _ => None,
        }
    }
}

/// What of `aggregation`, the aggregation of `view`, is not answered here, named: a field read
/// through a reference beside it, or a measure whose `where:` is not a filter [`unsupported`]
/// admits.
fn unread_aggregation(
    ir: &EssIr,
    view: &ResolvedView,
    aggregation: &ResolvedAggregation,
) -> Option<String> {
    if reads_related(view) {
        return Some("an aggregation beside a field read through a reference".to_owned());
    }
    aggregation
        .functions
        .iter()
        .find_map(|(name, measure)| {
            measure
                .r#where
                .as_ref()
                .filter(|condition| !reads(ir, view, condition))
                .map(|condition| (name, condition))
        })
        .map(|(name, condition)| {
            format!(
                "the aggregate `{name}` read `where: {condition}`; a measure's `where:` is read \
                 where it is a filter that is read"
            )
        })
}

/// Whether every leaf of `filter` is one [`unsupported`] admits. Each path is typed as ESS types
/// it ([`ess_compiler::expression::resolve_path`]): a field against the source's observable
/// fields, a field read through a reference (`<via>.<field>`) against the observable fields of the
/// entity it names, a parameter against the view's, and a binder as the element (`<list>.0`) of
/// the list it walks. ESS evaluates a quantifier over the elements a list publishes
/// (`<name>.count`, `<name>.<index>`). The parser bounds the nesting at `MAX_PREDICATE_DEPTH`.
fn reads(ir: &EssIr, view: &ResolvedView, filter: &Predicate) -> bool {
    let fields = ir.entity(&view.source).observable_fields();
    let scope = Scope {
        ir,
        view,
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

/// What a leaf of a filter may name where it stands: the source's fields, the fields of the rows
/// the view's references name (`view`), the view's parameters and the binders of the quantifiers
/// it is inside, innermost last.
struct Scope<'a> {
    ir: &'a EssIr,
    view: &'a ResolvedView,
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

    /// What `path` names here, or `None` where it is no field of the source, none of a row a
    /// reference names read through it (`<via>.<field>`, a binder's name never a `<via>`), no
    /// declared parameter and no binder in scope.
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
            [root, member] if !self.binders.iter().any(|binder| binder.name == root) => {
                let (reference, _) = self.view.navigated(self.ir, path)?;
                let far = self.ir.entity(&reference.entity).observable_fields();
                Some(Typed {
                    row: true,
                    binder: false,
                    resolution: self.resolve(&far, &FactPath::from_segments([member.clone()]))?,
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
                op,
                right: Operand::Fact(right),
                ..
            } => match (self.typed(left), self.typed(right)) {
                (Some(left), Some(right)) => {
                    (left.row || right.row)
                        && (!op.needs_ordering()
                            || (ordered(&left.resolution)
                                && ordered(&right.resolution)
                                && left.resolution.scalar == right.resolution.scalar))
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
                    view: self.view,
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
/// fields, the parameters under `param.`, typed by the view's, and a path `<via>.<member>` the
/// view navigates ([`ResolvedView::navigated`]) in the row the reference `via` names, typed by its
/// entity's observable fields, as ESS's interpreter reads a view's filter (`ess-conformance`
/// `src/interpret/views.rs` at ESS 0.92.0, `ViewFacts` over `TypedFacts`): a `Timestamp` is
/// ordered by the instant it names, any text but a `Duration` by its bytes, and a `now` operand
/// against `now`, the instant of the read (`ViewFacts::now`). A path read through a reference
/// that names no row reads nothing: no value, absent, and ordered neither as an instant nor by
/// its bytes, as `ViewFacts::source` answers it.
struct Read<'a> {
    ir: &'a EssIr,
    view: &'a ResolvedView,
    fields: &'a [ResolvedField],
    params: &'a [ResolvedField],
    row: &'a FactStore,
    parameters: &'a FactStore,
    navigated: &'a Navigated,
    now: Option<Rfc3339Instant>,
}

/// The rows a filter reads through the view's references, by the reference's source field
/// (`via`): the observable fields of the entity it names, and the row's facts where it names one.
type Navigated = BTreeMap<String, (Vec<ResolvedField>, Option<FactStore>)>;

impl Read<'_> {
    /// Where `path` is read, the fields that type it there, and its path there; `None` where it
    /// reads through a reference that names no row.
    fn source<'p>(
        &self,
        path: &'p FactPath,
    ) -> Option<(&FactStore, &[ResolvedField], std::borrow::Cow<'p, FactPath>)> {
        if let Some(inner) = parameter(path) {
            return Some((self.parameters, self.params, std::borrow::Cow::Owned(inner)));
        }
        if let Some((reference, _)) = self.view.navigated(self.ir, path) {
            let (fields, facts) = self.navigated.get(&reference.via)?;
            return Some((
                facts.as_ref()?,
                fields,
                std::borrow::Cow::Owned(FactPath::from_segments(&path.segments()[1..])),
            ));
        }
        Some((self.row, self.fields, std::borrow::Cow::Borrowed(path)))
    }

    /// The primitive `path` is declared as, through every newtype and `Optional`, and through a
    /// list to its element at an ordinal; `None` for anything else, `state` included. The outer
    /// `None` where `path` reads through a reference that names no row.
    fn declared(&self, path: &FactPath) -> Option<Option<Primitive>> {
        let (_, fields, path) = self.source(path)?;
        Some(
            match ess_compiler::expression::resolve_path(self.ir, fields, &path, "a view filter")
                .ok()
                .map(|resolved| resolved.terminal)
            {
                Some(ResolvedTypeRef::Primitive { name }) => Some(name),
                _ => None,
            },
        )
    }

    /// Whether ESS's interpreter hides the row rather than refusing the read, where the filter is
    /// unknown for it: every path it reads that observes nothing reads through a reference that
    /// names no row (`ess-conformance` `src/interpret/views.rs` at ESS 0.92.0,
    /// `absent_navigation`).
    fn absent_navigation(&self, filter: &Predicate) -> bool {
        let mut unobserved = filter
            .fact_paths()
            .into_iter()
            .filter(|path| self.observe(path).is_none())
            .peekable();
        unobserved.peek().is_some()
            && unobserved.all(|path| {
                self.view
                    .navigated(self.ir, path)
                    .is_some_and(|(reference, _)| {
                        self.navigated
                            .get(&reference.via)
                            .is_none_or(|(_, facts)| facts.is_none())
                    })
            })
    }
}

impl FactSource for Read<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        let (source, _, path) = self.source(path)?;
        source.fact(&path)
    }

    fn present(&self, path: &FactPath) -> bool {
        self.source(path)
            .is_some_and(|(source, _, path)| source.present(&path))
    }

    fn scales(&self) -> &ess_primitives::facts::Scales {
        self.row.scales()
    }

    fn orders_as_instant(&self, path: &FactPath) -> bool {
        self.declared(path) == Some(Some(Primitive::Timestamp))
    }

    fn orders_text_by_bytes(&self, path: &FactPath) -> bool {
        self.declared(path)
            .is_some_and(|declared| declared != Some(Primitive::Duration))
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
/// one, where ESS evaluates the filter to true over the row, `parameters` ([`parameters`]) and
/// `referenced`, the rows its references name ([`references`]). Only a filter [`unsupported`]
/// admits is read.
///
/// A view without parameters whose filter reads nothing but the state reads the state alone;
/// any other binds the state, the identity and every stored field, as ESS binds a source row
/// ([`ess_conformance::input::bind`] over the entity's observable fields), and each row a
/// reference names the same way over its own entity's. A field the row holds no value for is
/// absent, as ESS reads it: a comparison with it is unknown, `defined` false. A path read through
/// a reference that names no row reads nothing. A `now` operand is read as `now`, the instant of
/// the read the caller took ([`reads_now`]); with none it is unknown, as ESS reads it without a
/// clock.
///
/// A filter unknown for the row is refused, as ESS's interpreter refuses the read
/// (`ess-conformance` `src/interpret/views.rs:214` at ESS 0.92.0, `select`,
/// `Truth::Unknown => return Err(unsupported())`): one comparing against an `Optional` parameter
/// the read left out, or an `Optional` field the row does not hold, is unknown, not false. The
/// row is not held, and the read goes on, where every path the filter reads that observes nothing
/// reads through a reference that names no row (`absent_navigation`, `views.rs:213`). The
/// interpreter also hides a row where the omitted parameter is read by a string operator
/// (`absent_text_operands`, `views.rs:210`), and no filter [`unsupported`] admits has one.
///
/// # Errors
/// When the row, or a row a reference names, is not a value of its entity's declared fields,
/// naming what it holds, or when the filter is unknown for it.
pub(crate) fn holds(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
    referenced: &Referenced<'_>,
    parameters: &FactStore,
    now: Option<Rfc3339Instant>,
) -> Result<bool, String> {
    let Some(filter) = &view.filter else {
        return Ok(true);
    };
    let state_alone = view.params.is_empty()
        && filter
            .fact_paths()
            .iter()
            .all(|path| path.segments() == [STATE]);
    let (facts, navigated) = if state_alone {
        let mut facts = FactStore::new();
        if let Ok(path) = FactPath::new(STATE) {
            facts.set(path, FactValue::text(row.state.clone()));
        }
        (facts, Navigated::new())
    } else {
        (
            bound(ir, view, &source_row(ir, view, identity, row)?)?,
            navigated(ir, view, referenced)?,
        )
    };
    match truth(ir, view, filter, &facts, parameters, &navigated, now) {
        (Truth::True, _) => Ok(true),
        (Truth::False, _) | (Truth::Unknown, true) => Ok(false),
        (Truth::Unknown, false) => Err(format!(
            "the filter `{filter}` is unknown for a row, so the model does not determine the rows \
             it holds"
        )),
    }
}

/// The source row `row`, the instance `identity` names, as ESS's interpreter reads one
/// (`ess-conformance` `src/interpret/views.rs:170-172` at ESS 0.92.0, `select`): every stored
/// field it holds a value for, its identity and its `state`, as ESS values. A field it holds
/// `null` in is absent.
fn source_row(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
) -> Result<BTreeMap<String, Node>, String> {
    nodes(ir, view, source_json(ir, view, identity, row))
}

/// The source row [`source_row`] reads, as the store holds it.
fn source_json(ir: &EssIr, view: &ResolvedView, identity: &Value, row: &Row) -> Map<String, Value> {
    held_json(ir.entity(&view.source), identity, row)
}

/// `row`, a row of `entity` the instance `identity` names, as the store holds it: every stored
/// field it holds a value for, its identity and its `state`.
fn held_json(entity: &ResolvedEntity, identity: &Value, row: &Row) -> Map<String, Value> {
    let mut held = row
        .fields
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<Map<String, Value>>();
    held.insert(entity.identity.name.clone(), identity.clone());
    held.insert(STATE.to_owned(), Value::String(row.state.clone()));
    held
}

/// Each reference of `view` with the observable fields of the entity it names and, where
/// `referenced` holds the row it names, that row bound to them as ESS's interpreter binds it
/// (`ess-conformance` `src/interpret/views.rs:175-189` at ESS 0.92.0, `select`: its stored fields,
/// its identity and its `state`, [`ess_conformance::input::bind`]).
///
/// # Errors
/// Where a row a reference names is not a value of its entity's declared fields, as the
/// interpreter refuses the read.
fn navigated(
    ir: &EssIr,
    view: &ResolvedView,
    referenced: &Referenced<'_>,
) -> Result<Navigated, String> {
    view.references
        .iter()
        .map(|reference| {
            let entity = ir.entity(&reference.entity);
            let fields = entity.observable_fields();
            let facts = referenced
                .get(reference.via.as_str())
                .map(|(identity, row)| {
                    let held: BTreeMap<String, Node> =
                        serde_json::from_value(Value::Object(held_json(entity, identity, row)))
                            .map_err(|error| format!("a row of {}: {error}", entity.name))?;
                    ess_conformance::input::bind(ir, &fields, &held, Completeness::Partial)
                        .map_err(|errors| format!("a row of {}: {errors}", entity.name))
                })
                .transpose()?;
            Ok((reference.via.clone(), (fields, facts)))
        })
        .collect()
}

/// `held`, a source row of `view` as the store holds it, as ESS values.
fn nodes(
    ir: &EssIr,
    view: &ResolvedView,
    held: Map<String, Value>,
) -> Result<BTreeMap<String, Node>, String> {
    serde_json::from_value(Value::Object(held))
        .map_err(|error| format!("a row of {}: {error}", ir.entity(&view.source).name))
}

/// `fields`, a source row ([`source_row`]), bound to the source's observable fields as ESS binds
/// one ([`ess_conformance::input::bind`]).
fn bound(
    ir: &EssIr,
    view: &ResolvedView,
    fields: &BTreeMap<String, Node>,
) -> Result<FactStore, String> {
    let entity = ir.entity(&view.source);
    ess_conformance::input::bind(
        ir,
        &entity.observable_fields(),
        fields,
        Completeness::Partial,
    )
    .map_err(|errors| format!("a row of {}: {errors}", entity.name))
}

/// What ESS evaluates `predicate`, a filter [`unsupported`] admits, to over the row `facts`,
/// `parameters` and the rows its references name (`navigated`), a `now` operand read as `now`
/// ([`Read`]); and, where it is unknown, whether it is unknown only through a reference that
/// names no row ([`Read::absent_navigation`]).
fn truth(
    ir: &EssIr,
    view: &ResolvedView,
    predicate: &Predicate,
    facts: &FactStore,
    parameters: &FactStore,
    navigated: &Navigated,
    now: Option<Rfc3339Instant>,
) -> (Truth, bool) {
    let fields = ir.entity(&view.source).observable_fields();
    let read = Read {
        ir,
        view,
        fields: &fields,
        params: &view.params,
        row: facts,
        parameters,
        navigated,
        now,
    };
    let truth = predicate.evaluate(&read);
    (
        truth,
        truth == Truth::Unknown && read.absent_navigation(predicate),
    )
}

/// The rows of `view`, an aggregate view (`aggregation`), over `held`, each an identity and the
/// source row it names that the view's filter holds ([`holds`]), as ESS's interpreter answers them
/// (`ess-conformance` `src/interpret/views.rs` at ESS 0.92.0, `select` at 136-241 and `project`
/// at 370-432):
///
/// - each measure's `where:` is evaluated over each held row, after the filter
///   (`views.rs:217-225`); one unknown for a row refuses the read;
/// - the rows are grouped by the values of the `group_by` fields (`views.rs:383-401`), read from
///   the source row by name; one the row holds nothing for is `null` where the field is
///   `Optional` and refuses the read otherwise, and each is checked against its field's type as
///   ESS checks it ([`spindle_kernel::admits`]). Without `group_by` there is one group, held even
///   over no row (`views.rs:380-382`), so such a view always answers one row; with it, a group is
///   a key some held row has, and over no row there is none;
/// - the groups come in the order of their keys as ESS orders its values (`views.rs:378`, a
///   `BTreeMap` over `Node`). An aggregate view declares no `order_by` (`ess-domain`
///   `view.rs:1995`), so ESS's sort of the rows (`views.rs:68`) keeps that order;
/// - each group's row holds its key fields and every aggregate, computed by ESS's own measure
///   (`views.rs:404-430`): the value each member a measure reads holds
///   ([`ess_conformance::aggregate::measured`]), refused where a required input is missing, and
///   the function over them ([`ess_conformance::aggregate::evaluate_measure`]): `count` and a
///   required `sum` `0` over no value, an exact `sum`, an `avg` rounded to six places half-even
///   and absent over no value, a `sum` skipping absent values absent over none.
///
/// Each value is answered as [`answer`] writes it.
///
/// # Errors
/// Naming what refuses the read: a row that is not a value of the source's fields, a `where:`
/// unknown for a row, a group key the row holds no value of its type for, or a measure ESS
/// determines no value of.
pub(crate) fn aggregate(
    ir: &EssIr,
    view: &ResolvedView,
    aggregation: &ResolvedAggregation,
    held: &[(Value, Row)],
    parameters: &FactStore,
    now: Option<Rfc3339Instant>,
) -> Result<Vec<Map<String, Value>>, String> {
    type Members = Vec<(BTreeMap<String, Node>, BTreeSet<String>)>;
    let entity = ir.entity(&view.source);
    let conditions = aggregation
        .functions
        .iter()
        .filter_map(|(name, measure)| measure.r#where.as_ref().map(|at| (name, at)))
        .collect::<Vec<_>>();
    let mut groups: BTreeMap<Vec<Node>, Members> = BTreeMap::new();
    if aggregation.group_by.is_empty() {
        groups.insert(Vec::new(), Vec::new());
    }
    for (identity, row) in held {
        let stored = source_json(ir, view, identity, row);
        let fields = nodes(ir, view, stored.clone())?;
        let mut admitted = BTreeSet::new();
        if !conditions.is_empty() {
            let facts = bound(ir, view, &fields)?;
            for &(name, condition) in &conditions {
                match truth(
                    ir,
                    view,
                    condition,
                    &facts,
                    parameters,
                    &Navigated::new(),
                    now,
                )
                .0
                {
                    Truth::True => {
                        admitted.insert(name.clone());
                    }
                    Truth::False => {}
                    Truth::Unknown => {
                        return Err(format!(
                            "the aggregate `{name}` reads `where: {condition}`, which is unknown \
                             for a row, so the model does not determine its value"
                        ));
                    }
                }
            }
        }
        let mut key = Vec::new();
        for name in &aggregation.group_by {
            let field = view
                .fields
                .iter()
                .find(|field| &field.name == name)
                .ok_or_else(|| format!("the group key `{name}` is no field of {}", view.name))?;
            let value = match fields.get(name) {
                Some(value) => value.clone(),
                None if field.type_ref.is_optional() => Node::Null,
                None => {
                    return Err(format!(
                        "a row of {} holds no value of the group key `{name}`",
                        entity.name
                    ));
                }
            };
            spindle_kernel::admits(
                ir,
                &field.type_ref,
                stored.get(name).unwrap_or(&Value::Null),
            )
            .map_err(|error| format!("the group key `{name}`: {error}"))?;
            key.push(value);
        }
        groups.entry(key).or_default().push((fields, admitted));
    }
    let mut rows = Vec::new();
    for (key, members) in groups {
        let mut row: BTreeMap<&str, Node> = aggregation
            .group_by
            .iter()
            .map(String::as_str)
            .zip(key)
            .collect();
        for (name, measure) in &aggregation.functions {
            let mut values = Vec::new();
            for (fields, admitted) in &members {
                if measure.r#where.is_some() && !admitted.contains(name) {
                    continue;
                }
                if let Some(input) = &measure.input
                    && !fields.contains_key(&input.name)
                    && !input.type_ref.is_optional()
                {
                    return Err(format!(
                        "a row of {} holds no value of `{}`, which the aggregate `{name}` reads",
                        entity.name, input.name
                    ));
                }
                values.push(
                    ess_conformance::aggregate::measured(ir, entity, measure, fields).ok_or_else(
                        || format!("the model determines no value the aggregate `{name}` reads"),
                    )?,
                );
            }
            let kind = ess_conformance::aggregate::measure_kind(ir, entity, measure);
            let value = ess_conformance::aggregate::evaluate_measure(
                measure.function,
                &values,
                &kind,
                measure.skip_absent,
            )
            .ok_or_else(|| format!("the model determines no value of the aggregate `{name}`"))?;
            row.insert(name.as_str(), value);
        }
        rows.push(answer(row)?);
    }
    Ok(rows)
}

/// `row`, one row of an aggregate view as ESS values, as JSON: a number ESS spells as an integer
/// (a `count`, an `Integer` `sum`, an integral `Decimal`) as that integer, any other value as ESS
/// writes it ([`Node`]'s serialisation, a `Decimal` as its binary64, as the kernel writes one).
/// Every field is answered, an absent one (an `avg` over no value, an `Optional` group key the
/// group's rows hold nothing for) as `null`, as ESS's interpreter writes an aggregate row whatever
/// the field's presence (`ess-conformance` `src/interpret/views.rs:393` and `:429` at ESS 0.92.0)
/// and as its suite expects one (`<view>/aggregate`, `rating = null`).
///
/// # Errors
/// Naming the field whose value ESS writes no JSON of.
fn answer(row: BTreeMap<&str, Node>) -> Result<Map<String, Value>, String> {
    let mut answered = Map::new();
    for (name, value) in row {
        let json = match &value {
            Node::Null => Value::Null,
            Node::Number(number) => match number.as_i64() {
                Some(integer) => Value::from(integer),
                None => serde_json::to_value(&value)
                    .map_err(|error| format!("the aggregate `{name}`: {error}"))?,
            },
            _ => serde_json::to_value(&value)
                .map_err(|error| format!("the field `{name}`: {error}"))?,
        };
        answered.insert(name.to_owned(), json);
    }
    Ok(answered)
}

/// The rows the references of one source row name, as they stand now, by the source field each
/// reference reads (`via`): the referenced row's identity and the row. A reference that is
/// absent, `null`, or names no row has no entry ([`references`]).
pub(crate) type Referenced<'a> = BTreeMap<&'a str, (&'a Value, &'a Row)>;

/// Whether `view` declares a field read through a reference (`field: {related: …}`).
pub(crate) fn reads_related(view: &ResolvedView) -> bool {
    view.references
        .iter()
        .any(|reference| !reference.fields.is_empty())
}

/// Each reference of `view`, one a field reads through or its filter navigates
/// ([`ResolvedView::reads_through`]), with the entity it names and the identity the source row
/// `row`, the instance `identity` names, holds in it: by the reference's source field (`via`), in
/// the order the compiler resolved them. A reference whose field the row holds nothing or `null`
/// in names no row and is left out, as ESS's interpreter reads it (`ess-conformance`
/// `src/interpret/views.rs`, `referenced_rows`, at ESS 0.92.0).
pub(crate) fn references<'v>(
    ir: &'v EssIr,
    view: &'v ResolvedView,
    identity: &Value,
    row: &Row,
) -> Vec<(&'v str, &'v ResolvedEntity, Value)> {
    let source = ir.entity(&view.source);
    view.references
        .iter()
        .filter_map(|reference| {
            let named = if reference.via == source.identity.name {
                Some(identity.clone())
            } else {
                row.fields
                    .get(&reference.via)
                    .filter(|value| !value.is_null())
                    .cloned()
            }?;
            Some((reference.via.as_str(), ir.entity(&reference.entity), named))
        })
        .collect()
}

/// The view's row for the instance `identity` names, held as `row`; `referenced` holds the rows
/// its references name ([`references`]).
///
/// A field read through a reference answers what it reads of the row the reference names: its
/// `state`, its identity, or the stored field of that name, as ESS's interpreter reads it. A
/// field typed as the source's lifecycle state answers the state the row holds; the field named
/// as the source's identity answers `identity`; every other field answers the stored field of that
/// name. Where the row, or the referenced row, holds nothing for it, or the reference names no
/// row, the field follows the presence the compiler carried onto it from its source field: `null`
/// under `null_when_absent`, left out otherwise. A field read through a reference carries none, so
/// it is left out, which ESS reads as the `null` its interpreter answers.
///
/// A computed field (`expr:`) answers what ESS's evaluator computes over the source row, as
/// ESS's interpreter computes it (`ess-conformance` `src/interpret/views.rs:436-447` at ESS
/// 0.92.0, `project` and `computed`): each path it reads is the stored field of that name, the
/// identity or the `state`, one the row holds nothing or `null` in absent. It is computed and
/// spelled by the kernel's one computation of a value expression ([`spindle_kernel::compute`]),
/// the one an outcome's `{expr: …}` is written with: an `Integer` as a JSON integer, any other
/// value as ESS writes it. An absent value, a zero divisor outside `coalesce` among them, is
/// answered `null` whatever the field's presence, as ESS's interpreter writes it.
///
/// # Errors
/// Naming the computed field, where ESS's interpreter refuses the read: its value is unknown or
/// has no spelling of its type (an `Integer` past `i64`), it is absent and the field is not
/// `Optional`, or it is not a value of the field's type ([`spindle_kernel::admits`]).
pub(crate) fn project(
    ir: &EssIr,
    view: &ResolvedView,
    identity: &Value,
    row: &Row,
    referenced: &Referenced<'_>,
) -> Result<Map<String, Value>, String> {
    let entity: &ResolvedEntity = ir.entity(&view.source);
    let mut answer = Map::new();
    let state = Value::String(row.state.clone());
    for field in &view.fields {
        if let Some(expr) = view.computed.get(&field.name) {
            let read = |path: &FactPath| -> Option<Option<&Value>> {
                let name = path.to_string();
                Some(if name == entity.identity.name {
                    Some(identity)
                } else if name == STATE {
                    Some(&state)
                } else {
                    row.fields.get(&name)
                })
            };
            let refused = |why: &dyn std::fmt::Display| {
                format!(
                    "the computed field `{}`, the value `{expr}` computes for a row, {why}, so \
                     the model does not determine it",
                    field.name
                )
            };
            let value = match spindle_kernel::compute(ir, &field.type_ref, expr, &read) {
                Ok(Some(value)) => {
                    spindle_kernel::admits(ir, &field.type_ref, &value).map_err(|error| {
                        refused(&format_args!("is not a value of its type: {error}"))
                    })?;
                    value
                }
                Ok(None) if field.type_ref.is_optional() => Value::Null,
                Ok(None) => return Err(refused(&"is absent and the field is not Optional")),
                Err(why) => return Err(refused(&why)),
            };
            answer.insert(field.name.clone(), value);
            continue;
        }
        let value = if let Some((reference, read)) = view.related_read(&field.name) {
            referenced
                .get(reference.via.as_str())
                .and_then(|(far_identity, far)| {
                    if read == STATE {
                        Some(Value::String(far.state.clone()))
                    } else if read == ir.entity(&reference.entity).identity.name {
                        Some((*far_identity).clone())
                    } else {
                        far.fields
                            .get(read)
                            .filter(|value| !value.is_null())
                            .cloned()
                    }
                })
        } else if field.type_ref.required().declared() == Some(&entity.state_type) {
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
    Ok(answer)
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
