//! The value one determined field takes: an event's payload field, an error's field, or a field
//! `sets:` writes.
//!
//! Only the sources Spindle's own specification and the `rally` example use,
//! `{append: input.<path>}`, and a field or the state of the row an input field names
//! (`{related: {via: input.<field>, field: …}}`), and what a set outcome changed
//! (`{count: changed}`, `{changed: …}`), and a value an expression computes (`{expr: …}`, outside
//! an error), are read here; the rest are refused by [`crate::supported`] before a command runs.
//! Numbers are ESS's own [`Number`], so `{increment: N}` adds exactly as ESS adds, an expression
//! is computed by ESS's own evaluator ([`Expr::evaluate`]), and a literal is typed by
//! [`EssIr::literal_primitive`], the one question every ESS consumer asks of a literal.

use std::collections::BTreeMap;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EntityHandle, EventHandle, ResolvedCommand, ResolvedOutcome, ResolvedPayloadField,
    ResolvedPayloadValue, ResolvedRelatedVia, ResolvedTypeRef,
};
use ess_domain::QualifiedName;
use ess_domain::types::Primitive;
use ess_primitives::expr::{Evaluated, Expr, Reading};
use ess_primitives::facts::{FactPath, Number};
use ess_primitives::node::Node;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::ExecuteError;
use crate::mint::Generator;
use crate::store::{Row, Store};

/// The rows the command's related values read, as the store held them before the command ran:
/// by the entity's name and the identity's JSON text, `None` where no row carries the identity.
pub(crate) type Related = BTreeMap<(String, String), Option<Row>>;

/// Reads, before anything is selected or written, every row a `{related: {via: input.<field>,
/// …}}` value of `command` names, in any outcome, struct members included: what those values read
/// is the store as it was before the outcome (ESS `interpret::execute::determined::Reads`, whose
/// read authority is the original store). An input that names no row, being absent or `null`,
/// reads nothing.
///
/// # Errors
/// [`ExecuteError::Store`] where the store does not read.
pub(crate) fn related(
    ir: &EssIr,
    command: &ResolvedCommand,
    input: &Map<String, Value>,
    store: &mut dyn Store,
) -> Result<Related, ExecuteError> {
    fn reads<'f>(
        fields: impl IntoIterator<Item = &'f ResolvedPayloadField>,
        found: &mut Vec<(&'f str, &'f EntityHandle)>,
    ) {
        for field in fields {
            match &field.value {
                ResolvedPayloadValue::RelatedField {
                    via: ResolvedRelatedVia::Input { field, .. },
                    through,
                    entity,
                    ..
                }
                | ResolvedPayloadValue::RelatedState {
                    via: ResolvedRelatedVia::Input { field, .. },
                    through,
                    entity,
                    ..
                } if through.is_empty() => found.push((field, entity)),
                ResolvedPayloadValue::Struct { fields } => reads(fields, found),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    for outcome in &command.outcomes {
        reads(
            outcome
                .sets
                .iter()
                .chain(outcome.payload.iter().flat_map(|payload| &payload.fields))
                .chain(&outcome.error_payload),
            &mut found,
        );
    }
    let mut related = Related::new();
    for (field, entity) in found {
        let Some(identity) = read_input(input, field) else {
            continue;
        };
        let entity = ir.entity(entity).name.to_string();
        let key = (entity, identity.to_string());
        if related.contains_key(&key) {
            continue;
        }
        let row = store.load(&key.0, &identity)?.map(|stored| stored.row);
        related.insert(key, row);
    }
    Ok(related)
}

/// What a determined field may read: the command's input, the addressed instance as it was
/// immediately before the outcome, and the rows its related values name, as they were then.
#[derive(Clone, Copy)]
pub(crate) struct Sources<'a, 'm> {
    pub(crate) ir: &'a EssIr,
    pub(crate) input: &'a Map<String, Value>,
    /// The attributes of the caller the command is sent as; `None` for no caller.
    pub(crate) caller: Option<&'a Map<String, Value>>,
    pub(crate) before: Option<&'a Row>,
    /// The rows the command's related values name, read before the command ran ([`related`]).
    pub(crate) related: &'a Related,
    /// Whether the fields are a declared error's: there, a field read from the instance is absent
    /// where no instance is held (the unknown-instance arm of a `wrong_state:` refusal, ESS
    /// `error_value`, beyond10x/ess#458) rather than a value nothing determines.
    pub(crate) error: bool,
    /// The identity this outcome minted for the instance it creates, under the identity field's
    /// name, where these fields are the top-level fields of the creating event: what
    /// `{generated: true}` in a field of that name reads there, and nowhere else.
    pub(crate) minted: Option<(&'a str, &'a Value)>,
    /// The identity this outcome minted, with the event that publishes it, whatever fields these
    /// are: what a `{payload: <Event>.<field>}` read of the creating event sees.
    pub(crate) creation: Option<(&'a EventHandle, &'a str, &'a Value)>,
    /// The outcome the fields are this run's of: where `{payload: <Event>.<field>}` finds the
    /// source it reads.
    pub(crate) outcome: &'a ResolvedOutcome,
    /// The entity, event or error whose fields these are, as a generated field's mint is asked
    /// for it.
    pub(crate) owner: &'a QualifiedName,
    /// The values this run mints for its generated fields, each once.
    pub(crate) generator: &'a Generator<'m>,
    /// How many rows a set outcome changed, where these are its fields: what `{count: changed}`
    /// reads. A `{changed: …}` field reads [`Self::before`], the changed row as it was before the
    /// outcome with its identity among its fields.
    pub(crate) changed: Option<usize>,
}

impl<'a> Sources<'a, '_> {
    /// The value `field` takes; `None` where it is absent.
    pub(crate) fn value(
        &self,
        field: &ResolvedPayloadField,
    ) -> Result<Option<Value>, ExecuteError> {
        match &field.value {
            ResolvedPayloadValue::InputField { field: read, .. } => {
                Ok(read_input(self.input, read))
            }
            ResolvedPayloadValue::Literal { value } => literal(self.ir, field, value).map(Some),
            ResolvedPayloadValue::SubjectField { .. }
            | ResolvedPayloadValue::SubjectState { .. }
                if self.error && self.before.is_none() =>
            {
                Ok(None)
            }
            ResolvedPayloadValue::SubjectField { field: read, .. } => Ok(self
                .before(field)?
                .fields
                .get(read)
                .filter(|value| !value.is_null())
                .cloned()),
            ResolvedPayloadValue::SubjectState { .. } => {
                Ok(Some(Value::String(self.before(field)?.state.clone())))
            }
            // ESS `emit` holds the changed row an occurrence is for as the row before the outcome.
            ResolvedPayloadValue::ChangedField { field: read, .. } => Ok(self
                .before(field)?
                .fields
                .get(read)
                .filter(|value| !value.is_null())
                .cloned()),
            ResolvedPayloadValue::ChangedState { .. } => {
                Ok(Some(Value::String(self.before(field)?.state.clone())))
            }
            ResolvedPayloadValue::ChangedCount => self
                .changed
                .map(|count| Some(Value::from(count)))
                .ok_or_else(|| ExecuteError::NoValue {
                    what: format!(
                        "`{}`, which counts the rows changed by an outcome that changes no set",
                        field.target
                    ),
                }),
            ResolvedPayloadValue::Expression { expr, .. } if !self.error => {
                self.expression(field, expr)
            }
            ResolvedPayloadValue::Increment { by } => self.increment(field, by).map(Some),
            ResolvedPayloadValue::Append { field: read, .. } => self.append(field, read).map(Some),
            ResolvedPayloadValue::Generated { .. }
                if self
                    .minted
                    .is_some_and(|(identity, _)| identity == field.target) =>
            {
                Ok(self.minted.map(|(_, value)| value.clone()))
            }
            ResolvedPayloadValue::Generated { present } => self.generated(field, *present),
            ResolvedPayloadValue::InputOrGenerated {
                field: read,
                otherwise: None,
                ..
            } => match read_input(self.input, read) {
                Some(value) => Ok(Some(value)),
                None => self.generated(field, false),
            },
            ResolvedPayloadValue::PayloadRead {
                event, field: read, ..
            } => {
                let source = self
                    .outcome
                    .payload
                    .iter()
                    .filter(|payload| payload.event == *event)
                    .flat_map(|payload| &payload.fields)
                    .find(|source| source.target == *read)
                    .ok_or_else(|| ExecuteError::NoValue {
                        what: format!(
                            "`{}`, which stores `{}.{read}` the outcome does not fill",
                            field.target,
                            event.name()
                        ),
                    })?;
                Sources {
                    owner: event.name(),
                    error: false,
                    minted: self
                        .creation
                        .filter(|(creating, _, _)| *creating == event)
                        .map(|(_, name, value)| (name, value)),
                    ..*self
                }
                .value(source)
            }
            ResolvedPayloadValue::CallerAttribute {
                attribute,
                type_ref,
            } => self.caller_value(attribute, type_ref),
            ResolvedPayloadValue::RelatedField {
                via: ResolvedRelatedVia::Input { field: via, .. },
                through,
                entity,
                field: read,
                ..
            } if through.is_empty() => self.related(field, via, entity, Some(read)),
            ResolvedPayloadValue::RelatedState {
                via: ResolvedRelatedVia::Input { field: via, .. },
                through,
                entity,
                ..
            } if through.is_empty() => self.related(field, via, entity, None),
            ResolvedPayloadValue::EmptyList => Ok(Some(Value::Array(Vec::new()))),
            ResolvedPayloadValue::Cleared => Ok(None),
            ResolvedPayloadValue::Struct { fields } => {
                // A member is never the minted identity: only the top-level field `instance:`
                // names on the creating event is (ESS `interpret::execute::emit`).
                let members_sources = Sources {
                    minted: None,
                    ..*self
                };
                let mut members = Map::new();
                for member in fields {
                    if let Some(value) = members_sources.value(member)? {
                        members.insert(member.target.clone(), value);
                    }
                }
                Ok(Some(Value::Object(members)))
            }
            other => Err(ExecuteError::Unsupported {
                construct: format!("the value source {}", other.describe()),
            }),
        }
    }

    /// Every field in `fields` that holds a value, by target.
    pub(crate) fn object(
        &self,
        fields: &[ResolvedPayloadField],
    ) -> Result<Map<String, Value>, ExecuteError> {
        let mut object = Map::new();
        for field in fields {
            if let Some(value) = self.value(field)? {
                object.insert(field.target.clone(), value);
            }
        }
        Ok(object)
    }

    /// The value this run mints for the generated `field`.
    fn generated(
        &self,
        field: &ResolvedPayloadField,
        present: bool,
    ) -> Result<Option<Value>, ExecuteError> {
        self.generator.generated(
            self.ir,
            self.outcome.name.as_str(),
            self.owner,
            field,
            present,
        )
    }

    /// `{caller: <attribute>}` (ess/16), as ESS reads it (`interpret::execute::caller::Caller::
    /// value`): the caller's value, absent where the caller carries none and the attribute is
    /// `Optional`. A required attribute the command is sent without is a request that cannot be
    /// answered, refused before anything is written.
    fn caller_value(
        &self,
        attribute: &str,
        type_ref: &ResolvedTypeRef,
    ) -> Result<Option<Value>, ExecuteError> {
        match self
            .caller
            .and_then(|caller| caller.get(attribute))
            .filter(|value| !value.is_null())
        {
            Some(value) => {
                crate::typed::validate(self.ir, type_ref, value).map_err(|error| {
                    ExecuteError::InvalidInput(format!("the caller's `{attribute}`: {error}"))
                })?;
                Ok(Some(value.clone()))
            }
            None if type_ref.is_optional() => Ok(None),
            None => Err(ExecuteError::InvalidInput(format!(
                "the command reads the caller attribute `{attribute}`, and it is sent as no \
                 caller that carries it"
            ))),
        }
    }

    /// `{related: {via: input.<via>, field: <read>}}` (ess/16), or the held state where `read` is
    /// `None` (ess/28), as ESS reads it (`interpret::execute::determined::Reads::related`): the
    /// field of the `entity` row the input names, as the store held it before the command ran.
    /// An `Optional` input left absent names no row and the value is absent; an identity no row
    /// carries determines nothing. The identity field reads the identity itself.
    fn related(
        &self,
        field: &ResolvedPayloadField,
        via: &str,
        entity: &EntityHandle,
        read: Option<&str>,
    ) -> Result<Option<Value>, ExecuteError> {
        let Some(identity) = read_input(self.input, via) else {
            return Ok(None);
        };
        let entity = self.ir.entity(entity);
        let row = self
            .related
            .get(&(entity.name.to_string(), identity.to_string()))
            .and_then(Option::as_ref)
            .ok_or_else(|| ExecuteError::NoValue {
                what: format!(
                    "`{}`, which reads the `{}` row input.{via} names, and none carries {identity}",
                    field.target, entity.name
                ),
            })?;
        Ok(match read {
            None => Some(Value::String(row.state.clone())),
            Some(read) if read == entity.identity.name => Some(identity),
            Some(read) => row
                .fields
                .get(read)
                .filter(|value| !value.is_null())
                .cloned(),
        })
    }

    fn before(&self, field: &ResolvedPayloadField) -> Result<&'a Row, ExecuteError> {
        self.before.ok_or_else(|| ExecuteError::NoValue {
            what: format!("`{}`, which reads an instance none holds", field.target),
        })
    }

    fn increment(&self, field: &ResolvedPayloadField, by: &str) -> Result<Value, ExecuteError> {
        let no_value = || ExecuteError::NoValue {
            what: format!("the previous `{}` plus {by}", field.target),
        };
        let held = self
            .before(field)?
            .fields
            .get(&field.target)
            .filter(|value| !value.is_null())
            .ok_or_else(no_value)?;
        let previous = serde_json::from_value::<Number>(held.clone()).map_err(|_| no_value())?;
        let step = Number::decimal_literal(by).ok_or_else(no_value)?;
        let sum = previous.checked_add(step).ok_or_else(no_value)?;
        match self.ir.literal_primitive(&field.target_type) {
            Some(Primitive::Integer) => sum.as_i64().map(Value::from).ok_or_else(no_value),
            _ => serde_json::to_value(sum).map_err(|_| no_value()),
        }
    }

    /// `{expr: "<expression>"}` (ess/27, G1), as ESS computes it (`interpret::execute::determined::
    /// expression`): ESS's own evaluator over `input.<path>` from the command's input and
    /// `subject.<field>` from the instance as it was before the outcome, the result spelled as
    /// ESS spells it. An absent read, or a zero divisor outside `coalesce`, leaves the field
    /// absent. A read of an instance none holds, a value past a bound (an Integer past `i64`, an
    /// instant past what RFC 3339 spells) or one with no spelling determines nothing: the
    /// evaluator is exact and checked, and never wraps or saturates.
    fn expression(
        &self,
        field: &ResolvedPayloadField,
        expr: &Expr,
    ) -> Result<Option<Value>, ExecuteError> {
        let reads = |path: &FactPath| -> Reading {
            let text = path.to_string();
            if let Some(read) = text.strip_prefix("input.") {
                return borrowed_input(self.input, read).map_or(Reading::Absent, reading);
            }
            if let Some(read) = text.strip_prefix("subject.") {
                return match self.before {
                    Some(row) => row.fields.get(read).map_or(Reading::Absent, reading),
                    None => Reading::Unknown,
                };
            }
            Reading::Unknown
        };
        let no_value = |why: &str| ExecuteError::NoValue {
            what: format!(
                "`{}`, the value `{expr}` computes, which {why}",
                field.target
            ),
        };
        match expr.evaluate(&reads) {
            Evaluated::Unknown => Err(no_value(
                "reads an undetermined value or passes a bound of its type",
            )),
            evaluated => match evaluated.to_node() {
                Some(Node::Null) => Ok(None),
                // An `Integer` is written as the integer, as `{increment: …}` writes one; ESS's
                // `Number` writes an integral value as `2.0`, one number either way.
                Some(Node::Number(number))
                    if self.ir.literal_primitive(&field.target_type)
                        == Some(Primitive::Integer) =>
                {
                    number
                        .as_i64()
                        .map(|integer| Some(Value::from(integer)))
                        .ok_or_else(|| no_value("has no spelling of its type"))
                }
                Some(node) => serde_json::to_value(node)
                    .map(Some)
                    .map_err(|_| no_value("has no spelling of its type")),
                None => Err(no_value("has no spelling of its type")),
            },
        }
    }

    /// `{append: input.<read>}` (ess/31), as ESS determines it (`interpret::execute::determined::
    /// append`): the list the instance held in the target before the outcome, with the input
    /// element last. A held `null` reads as empty; a list the instance does not hold reads as
    /// empty where the target is `Optional<List<…>>` and determines nothing where it is required. Over a `List<Optional<T>>` an element
    /// the caller did not send appends `null`; over a list of required elements it determines
    /// nothing. The list is checked against the target's type where `sets:` is written.
    fn append(&self, field: &ResolvedPayloadField, read: &str) -> Result<Value, ExecuteError> {
        let no_value = || ExecuteError::NoValue {
            what: format!("the previous `{}` with input.{read} appended", field.target),
        };
        let optional_element = matches!(
            field.target_type.required(),
            ResolvedTypeRef::List { of } if of.is_optional()
        );
        let element = match read_input(self.input, read) {
            Some(element) => element,
            None if optional_element => Value::Null,
            None => return Err(no_value()),
        };
        let mut items = match self.before.and_then(|row| row.fields.get(&field.target)) {
            Some(Value::Array(items)) => items.clone(),
            Some(Value::Null) => Vec::new(),
            None if field.target_type.is_optional() => Vec::new(),
            _ => return Err(no_value()),
        };
        items.push(element);
        Ok(Value::Array(items))
    }
}

/// The input value at `read`: a field, or a member reached through `.`-separated segments.
/// `None` where a member on the way is missing, or the value is `null`.
pub(crate) fn read_input(input: &Map<String, Value>, read: &str) -> Option<Value> {
    let mut segments = read.split('.');
    let mut value = input.get(segments.next()?)?;
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    (!value.is_null()).then(|| value.clone())
}

/// [`read_input`], borrowed: an expression reads a scalar, and copies nothing else of the input.
fn borrowed_input<'v>(input: &'v Map<String, Value>, read: &str) -> Option<&'v Value> {
    let mut segments = read.split('.');
    let mut value = input.get(segments.next()?)?;
    for segment in segments {
        value = value.as_object()?.get(segment)?;
    }
    Some(value)
}

/// What an expression reads `value` as ([`Reading::of_node`]): `null` is absent, a scalar its
/// value, and a list or a mapping no operand, read as unknown without being copied.
fn reading(value: &Value) -> Reading {
    match value {
        Value::Array(_) | Value::Object(_) => Reading::Unknown,
        scalar => {
            Node::deserialize(scalar).map_or(Reading::Unknown, |node| Reading::of_node(&node))
        }
    }
}

/// The typed value a literal written in the model denotes, read against the field it fills: a
/// `Boolean`, `Integer` or `Decimal` as that value, everything else (text, an enum variant, a
/// state) as the text itself.
fn literal(ir: &EssIr, field: &ResolvedPayloadField, text: &str) -> Result<Value, ExecuteError> {
    let refused = || ExecuteError::Unsupported {
        construct: format!(
            "the literal `{text}` over `{}` in `{}`",
            field.target_type, field.target
        ),
    };
    match ir.literal_primitive(&field.target_type) {
        None => Ok(Value::String(text.to_owned())),
        Some(Primitive::Boolean) => match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(refused()),
        },
        Some(Primitive::Integer) => text
            .parse::<i64>()
            .ok()
            .filter(|number| number.to_string() == text)
            .map(Value::from)
            .ok_or_else(refused),
        Some(Primitive::Decimal) => Number::decimal_literal(text)
            .and_then(|number| serde_json::to_value(number).ok())
            .ok_or_else(refused),
        Some(_) => Err(refused()),
    }
}
