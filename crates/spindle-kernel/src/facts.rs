//! The facts a guard or an invariant reads, beside the fields that declare them.
//!
//! A guard is ESS's own [`Predicate`](ess_primitives::predicate::Predicate) and is evaluated by
//! [`Predicate::evaluate`](ess_primitives::predicate::Predicate::evaluate); this module only says
//! which value stands at which path and what type declares it. A stored field is read at its own
//! name (`ball_x`), the state at `state`, and an input field under `input.` (`input.side`), as
//! `when_subject:` reads them; an input guard reads the input fields at their own names. Either
//! reads the attributes of the caller the command is sent as under `caller.` (`caller.owner_id`).
//!
//! A JSON value binds its leaves the way every ESS fact source does: a scalar at its own path, an
//! object member at `<path>.<member>`, and a list as `<path>.count` with its elements at
//! `<path>.<index>`. A value whose declared type is a `Map` binds as a list does: `<path>.count`
//! and its values at `<path>.<index>` in key order, no key at any path, as ESS 0.99.1 projects one
//! (`input::project_value`). `null` binds nothing, so a guard over an absent value is `Unknown`,
//! never `false`.
//!
//! The declared types answer how text is ordered, as ESS's typed fact sources answer it: a
//! `Timestamp` by the instant it names, a `Duration` not at all, any other text by its UTF-8
//! bytes. A command guard reads the decision's one instant as `now` ([`Facts::at`]), and so does a
//! row-set selector ([`Selector`]). An invariant is read with no current time, as ESS's
//! interpreter reads one at rest, so a comparison with `now` there stays `Unknown`.

use std::collections::{BTreeMap, BTreeSet};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedEntity, ResolvedField, ResolvedTypeRef};
use ess_domain::types::Primitive;
use ess_primitives::facts::{FactPath, FactSource, FactStore, FactValue, Scales};
use ess_primitives::time::Rfc3339Instant;
use serde_json::{Map, Value};

use crate::store::Row;

/// The namespace a stored-field guard reads the command's input under.
pub(crate) const INPUT: &str = ess_domain::command::subject_fact::INPUT_NAMESPACE;

/// The namespace a guard reads the caller's attributes under.
pub(crate) const CALLER: &str = ess_domain::command::caller_value::CALLER_NAMESPACE;

/// The namespace a row-set selector reads the addressed subject under.
const SUBJECT: &str = ess_domain::command::set_effects::SUBJECT_NAMESPACE;

#[cfg(test)]
thread_local! {
    /// How many times [`Facts::row`] built a row's facts on this thread: what a test reads to
    /// tell that a path skipped them.
    pub(crate) static ROWS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Bound facts and the fields that declare them.
pub(crate) struct Facts<'a> {
    ir: &'a EssIr,
    store: FactStore,
    present: BTreeSet<FactPath>,
    /// The fields the paths at their own names are declared by.
    fields: Vec<ResolvedField>,
    /// The command input read under `input.`, where it is.
    input: Option<&'a [ResolvedField]>,
    /// The caller's attributes read under `caller.`, where they are.
    caller: Option<&'a [ResolvedField]>,
    /// The instant `now` reads, where a guard of a command decision reads these facts.
    now: Option<Rfc3339Instant>,
}

impl<'a> Facts<'a> {
    fn empty(ir: &'a EssIr, fields: Vec<ResolvedField>) -> Self {
        Self {
            ir,
            store: FactStore::new(),
            present: BTreeSet::new(),
            fields,
            input: None,
            caller: None,
            now: None,
        }
    }

    /// These facts read by a guard of a command decision at `now`, the decision's one instant, as
    /// ESS's interpreter reads `when:`, `when_subject:` and a `when_related:` row
    /// (`interpret::execute::caller::Facts::new(.., context.now)`). `None` binds no instant, so a
    /// comparison with `now` is `Unknown`.
    pub(crate) fn at(mut self, now: Option<Rfc3339Instant>) -> Self {
        self.now = now;
        self
    }

    /// These facts with the attributes of the caller the command is sent as, each at
    /// `caller.<attribute>` and declared by `attributes`, as ESS reads a guard's `caller.` operand
    /// (`interpret::execute::caller::Facts`). Where the facts' own fields declare one named
    /// `caller`, `caller.<member>` keeps reading that field, and nothing of the caller is bound.
    /// A command sent as no caller binds nothing, so a guard over the caller is `Unknown`.
    pub(crate) fn with_caller(
        mut self,
        attributes: &'a [ResolvedField],
        caller: Option<&Map<String, Value>>,
    ) -> Self {
        if self.fields.iter().any(|field| field.name == CALLER) {
            return self;
        }
        self.caller = Some(attributes);
        for (name, value) in caller.into_iter().flatten() {
            if let Ok(path) = FactPath::new(format!("{CALLER}.{name}")) {
                self.bind(&path, value, declared_by(attributes, name));
            }
        }
        self
    }

    /// The facts of `row`, an instance of `entity` with `identity`: its fields at their own
    /// names, its identity at the identity's, and its state at `state`. With `input`, the
    /// command's input is read under `input.` too.
    pub(crate) fn row(
        ir: &'a EssIr,
        entity: &ResolvedEntity,
        identity: &Value,
        row: &Row,
        input: Option<(&'a [ResolvedField], &Map<String, Value>)>,
    ) -> Self {
        #[cfg(test)]
        ROWS_BUILT.with(|built| built.set(built.get() + 1));
        let mut fields = entity.fields.clone();
        fields.push(entity.identity.clone());
        let mut facts = Self::empty(ir, fields);
        if let Ok(path) = FactPath::new("state") {
            facts.store.set(path, FactValue::text(row.state.clone()));
        }
        if let Ok(path) = FactPath::new(&entity.identity.name) {
            facts.bind(&path, identity, Some(&entity.identity.type_ref));
        }
        for (name, value) in &row.fields {
            if let Ok(path) = FactPath::new(name) {
                facts.bind(&path, value, declared_by(&entity.fields, name));
            }
        }
        if let Some((declared, values)) = input {
            facts.input = Some(declared);
            for (name, value) in values {
                if let Ok(path) = FactPath::new(format!("{INPUT}.{name}")) {
                    facts.bind(&path, value, declared_by(declared, name));
                }
            }
        }
        facts
    }

    /// The facts of `values`, each at the name of the field in `fields` that declares it: what an
    /// input guard reads of a command's input, and a struct's invariant of its members.
    pub(crate) fn values(
        ir: &'a EssIr,
        fields: &[ResolvedField],
        values: &Map<String, Value>,
    ) -> Self {
        let mut facts = Self::empty(ir, fields.to_vec());
        for (name, value) in values {
            if let Ok(path) = FactPath::new(name) {
                facts.bind(&path, value, declared_by(fields, name));
            }
        }
        facts
    }

    /// Binds `value` at `path` as the type `declared` shapes it, as ESS's
    /// `input::project_value` binds a typed value: a list and a map as `<path>.count` with their
    /// elements at `<path>.<index>`, a map's values in key order and no key at any path, a struct's
    /// members at `<path>.<member>`. A value no type declares binds as its JSON shape reads.
    fn bind(&mut self, path: &FactPath, value: &Value, declared: Option<&ResolvedTypeRef>) {
        let ir = self.ir;
        let shape = declared.and_then(|declared| Shape::of(ir, declared));
        match value {
            Value::Null => {}
            Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                if let Ok(fact) = serde_json::from_value::<FactValue>(value.clone()) {
                    self.store.set(path.clone(), fact);
                }
            }
            Value::Array(elements) => {
                let of = match shape {
                    Some(Shape::List(of)) => Some(of),
                    _ => None,
                };
                self.bind_list(path, elements.iter(), of);
            }
            Value::Object(members) => match shape {
                Some(Shape::Map(of)) => {
                    let mut entries = members.iter().collect::<Vec<_>>();
                    entries.sort_by_key(|(key, _)| *key);
                    self.bind_list(path, entries.into_iter().map(|(_, value)| value), Some(of));
                }
                shape => {
                    let fields = match shape {
                        Some(Shape::Struct(fields)) => fields,
                        _ => &[],
                    };
                    self.present.insert(path.clone());
                    for (member, element) in members {
                        self.bind(&path.child(member), element, declared_by(fields, member));
                    }
                }
            },
        }
    }

    /// Binds `<path>.count` and each of `elements` at `<path>.<index>`, as ESS's
    /// `input::project_list` binds a list or a map's values.
    fn bind_list<'v>(
        &mut self,
        path: &FactPath,
        elements: impl ExactSizeIterator<Item = &'v Value>,
        of: Option<&ResolvedTypeRef>,
    ) {
        self.present.insert(path.clone());
        self.store
            .set(path.child("count"), FactValue::count(elements.len()));
        for (index, element) in elements.enumerate() {
            self.bind(&path.child(&index.to_string()), element, of);
        }
    }

    /// The primitive `path` resolves to through any newtype or `Optional`; `None` for a path no
    /// field declares, such as `state`.
    fn declared(&self, path: &FactPath) -> Option<Primitive> {
        let segments = path.segments();
        let (fields, path) = match (self.input, self.caller, segments.split_first()) {
            (Some(input), _, Some((first, rest)))
                if first.as_str() == INPUT && !rest.is_empty() =>
            {
                (input, FactPath::from_segments(rest))
            }
            (_, Some(caller), Some((first, rest)))
                if first.as_str() == CALLER && !rest.is_empty() =>
            {
                (caller, FactPath::from_segments(rest))
            }
            _ => (self.fields.as_slice(), path.clone()),
        };
        match ess_compiler::expression::resolve_path(self.ir, fields, &path, "spindle facts") {
            Ok(resolved) => match resolved.terminal {
                ResolvedTypeRef::Primitive { name } => Some(name),
                _ => None,
            },
            Err(_) => None,
        }
    }
}

/// What a declared type makes of the JSON value it binds, through any `Optional` or newtype: the
/// shapes whose facts differ from the value's own JSON shape, as ESS's `input::project_value`
/// reads them.
enum Shape<'t> {
    /// A list of this element type.
    List(&'t ResolvedTypeRef),
    /// A map with values of this type, bound as a list of its values in key order.
    Map(&'t ResolvedTypeRef),
    /// A struct with these members.
    Struct(&'t [ResolvedField]),
}

impl<'t> Shape<'t> {
    fn of(ir: &'t EssIr, declared: &'t ResolvedTypeRef) -> Option<Self> {
        match declared {
            ResolvedTypeRef::Optional { of } => Self::of(ir, of),
            ResolvedTypeRef::List { of } => Some(Self::List(of)),
            ResolvedTypeRef::Map { value, .. } => Some(Self::Map(value)),
            ResolvedTypeRef::Declared { name } => match &ir.named_type(name).body {
                ResolvedBody::Newtype { of, .. } => Self::of(ir, of),
                ResolvedBody::Struct { fields, .. } => Some(Self::Struct(fields)),
                _ => None,
            },
            ResolvedTypeRef::Primitive { .. } => None,
        }
    }
}

/// The type the field `name` of `fields` declares.
fn declared_by<'t>(fields: &'t [ResolvedField], name: &str) -> Option<&'t ResolvedTypeRef> {
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.type_ref)
}

/// The facts a row-set selector reads of one candidate row, routed as ESS's interpreter routes
/// them (`interpret::execute::set_effects::Facts` inside `caller::Facts`): `input.<field>` to the
/// command's input, `subject.<field>` to the addressed subject as it was before the outcome, and
/// every other path to the candidate row, its fields and identity at their own names and its held
/// state at `state`. `now` is the decision's one instant. A selector is read as no caller, so
/// `caller.<attribute>` binds nothing. A rows quantifier (`exists: {in: {entity: …}, …}`, ess/27)
/// walks the rows of its entity that `walked` holds, and is `Unknown` where it holds none, as
/// ESS's `set_effects::Facts::rows` answers.
pub(crate) struct Selector<'a> {
    /// The candidate row ([`Facts::row`], without the input).
    pub(crate) row: &'a Facts<'a>,
    /// The command's input at its fields' own names ([`Facts::values`]).
    pub(crate) input: &'a Facts<'a>,
    /// The addressed subject, where the command addresses one the store holds.
    pub(crate) subject: Option<&'a Facts<'a>>,
    /// The decision's instant.
    pub(crate) now: Option<Rfc3339Instant>,
    /// The stored rows of each entity a rows quantifier of the selector walks, where the reader
    /// read them ([`Walked`]).
    pub(crate) walked: Option<&'a Walked<'a>>,
}

/// The stored rows of each entity a selector's rows quantifier walks, each as [`Facts::row`] binds
/// it, by the entity's name as the quantifier writes it: read from the same store as the
/// candidates, before selection (ESS `interpret::execute::row_set::Reader::members`).
pub(crate) type Walked<'a> = BTreeMap<String, Vec<Facts<'a>>>;

/// One walked row, lent to the quantifier that walks it.
struct Lent<'a>(&'a Facts<'a>);

impl FactSource for Lent<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        self.0.fact(path)
    }

    fn observe(&self, path: &FactPath) -> Option<FactValue> {
        self.0.observe(path)
    }

    fn present(&self, path: &FactPath) -> bool {
        self.0.present(path)
    }

    fn observed_presence(&self, path: &FactPath) -> Option<bool> {
        self.0.observed_presence(path)
    }

    fn cardinality(&self, path: &FactPath) -> Option<usize> {
        self.0.cardinality(path)
    }

    fn scales(&self) -> &Scales {
        self.0.scales()
    }

    fn orders_as_instant(&self, path: &FactPath) -> bool {
        self.0.orders_as_instant(path)
    }

    fn orders_text_by_bytes(&self, path: &FactPath) -> bool {
        self.0.orders_text_by_bytes(path)
    }

    fn now(&self) -> Option<Rfc3339Instant> {
        self.0.now()
    }
}

impl Selector<'_> {
    fn source(&self, path: &FactPath) -> Option<(&dyn FactSource, FactPath)> {
        let (first, rest) = path.segments().split_first()?;
        if rest.is_empty() {
            return Some((self.row, path.clone()));
        }
        match first.as_str() {
            CALLER => None,
            INPUT => Some((self.input, FactPath::from_segments(rest))),
            SUBJECT => Some((self.subject?, FactPath::from_segments(rest))),
            _ => Some((self.row, path.clone())),
        }
    }
}

impl FactSource for Selector<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        self.source(path)
            .and_then(|(source, path)| source.fact(&path))
    }

    fn observe(&self, path: &FactPath) -> Option<FactValue> {
        self.source(path)
            .and_then(|(source, path)| source.observe(&path))
    }

    fn present(&self, path: &FactPath) -> bool {
        self.source(path)
            .is_some_and(|(source, path)| source.present(&path))
    }

    fn observed_presence(&self, path: &FactPath) -> Option<bool> {
        self.source(path).map_or(Some(false), |(source, path)| {
            source.observed_presence(&path)
        })
    }

    fn cardinality(&self, path: &FactPath) -> Option<usize> {
        self.source(path)
            .and_then(|(source, path)| source.cardinality(&path))
    }

    fn scales(&self) -> &Scales {
        self.input.scales()
    }

    fn orders_as_instant(&self, path: &FactPath) -> bool {
        self.source(path)
            .is_some_and(|(source, path)| source.orders_as_instant(&path))
    }

    fn orders_text_by_bytes(&self, path: &FactPath) -> bool {
        self.source(path)
            .is_some_and(|(source, path)| source.orders_text_by_bytes(&path))
    }

    fn now(&self) -> Option<Rfc3339Instant> {
        self.now
    }

    fn rows(&self, entity: &str) -> Option<Vec<Box<dyn FactSource + '_>>> {
        let rows = self.walked?.get(entity)?;
        Some(
            rows.iter()
                .map(|row| Box::new(Lent(row)) as Box<dyn FactSource + '_>)
                .collect(),
        )
    }
}

impl FactSource for Facts<'_> {
    fn fact(&self, path: &FactPath) -> Option<FactValue> {
        self.store.fact(path)
    }

    fn present(&self, path: &FactPath) -> bool {
        self.present.contains(path) || self.observe(path).is_some()
    }

    fn observed_presence(&self, path: &FactPath) -> Option<bool> {
        Some(self.present(path))
    }

    fn scales(&self) -> &Scales {
        self.store.scales()
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
