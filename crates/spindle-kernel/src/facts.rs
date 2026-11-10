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
//! `<path>.<index>`. `null` binds nothing, so a guard over an absent value is `Unknown`, never
//! `false`.
//!
//! The declared types answer how text is ordered, as ESS's typed fact sources answer it: a
//! `Timestamp` by the instant it names, a `Duration` not at all, any other text by its UTF-8
//! bytes. A command guard reads the decision's one instant as `now` ([`Facts::at`]), and so does a
//! row-set selector ([`Selector`]). An invariant is read with no current time, as ESS's
//! interpreter reads one at rest, so a comparison with `now` there stays `Unknown`.

use std::collections::BTreeSet;

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedEntity, ResolvedField, ResolvedTypeRef};
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
                self.bind(&path, value);
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
        let mut fields = entity.fields.clone();
        fields.push(entity.identity.clone());
        let mut facts = Self::empty(ir, fields);
        if let Ok(path) = FactPath::new("state") {
            facts.store.set(path, FactValue::text(row.state.clone()));
        }
        if let Ok(path) = FactPath::new(&entity.identity.name) {
            facts.bind(&path, identity);
        }
        for (name, value) in &row.fields {
            if let Ok(path) = FactPath::new(name) {
                facts.bind(&path, value);
            }
        }
        if let Some((declared, values)) = input {
            facts.input = Some(declared);
            for (name, value) in values {
                if let Ok(path) = FactPath::new(format!("{INPUT}.{name}")) {
                    facts.bind(&path, value);
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
                facts.bind(&path, value);
            }
        }
        facts
    }

    fn bind(&mut self, path: &FactPath, value: &Value) {
        match value {
            Value::Null => {}
            Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                if let Ok(fact) = serde_json::from_value::<FactValue>(value.clone()) {
                    self.store.set(path.clone(), fact);
                }
            }
            Value::Array(elements) => {
                self.present.insert(path.clone());
                self.store
                    .set(path.child("count"), FactValue::count(elements.len()));
                for (index, element) in elements.iter().enumerate() {
                    self.bind(&path.child(&index.to_string()), element);
                }
            }
            Value::Object(members) => {
                self.present.insert(path.clone());
                for (member, element) in members {
                    self.bind(&path.child(member), element);
                }
            }
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

/// The facts a row-set selector reads of one candidate row, routed as ESS's interpreter routes
/// them (`interpret::execute::set_effects::Facts` inside `caller::Facts`): `input.<field>` to the
/// command's input, `subject.<field>` to the addressed subject as it was before the outcome, and
/// every other path to the candidate row, its fields and identity at their own names and its held
/// state at `state`. `now` is the decision's one instant. A selector is read as no caller, so
/// `caller.<attribute>` binds nothing, and walks no rows.
pub(crate) struct Selector<'a> {
    /// The candidate row ([`Facts::row`], without the input).
    pub(crate) row: &'a Facts<'a>,
    /// The command's input at its fields' own names ([`Facts::values`]).
    pub(crate) input: &'a Facts<'a>,
    /// The addressed subject, where the command addresses one the store holds.
    pub(crate) subject: Option<&'a Facts<'a>>,
    /// The decision's instant.
    pub(crate) now: Option<Rfc3339Instant>,
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
