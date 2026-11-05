//! The value one determined field takes: an event's payload field, an error's field, or a field
//! `sets:` writes.
//!
//! Only the sources Spindle's own specification and the `rally` example use, and
//! `{append: input.<path>}`, are read here; the rest are refused by [`crate::supported`] before a
//! command runs. Numbers are ESS's own [`Number`], so `{increment: N}` adds exactly as ESS adds, and a literal is typed by
//! [`EssIr::literal_primitive`], the one question every ESS consumer asks of a literal.

use ess_compiler::EssIr;
use ess_compiler::ir::{
    EventHandle, ResolvedOutcome, ResolvedPayloadField, ResolvedPayloadValue, ResolvedTypeRef,
};
use ess_domain::QualifiedName;
use ess_domain::types::Primitive;
use ess_primitives::facts::Number;
use serde_json::{Map, Value};

use crate::ExecuteError;
use crate::mint::Generator;
use crate::store::Row;

/// What a determined field may read: the command's input and the addressed instance as it was
/// immediately before the outcome.
#[derive(Clone, Copy)]
pub(crate) struct Sources<'a, 'm> {
    pub(crate) ir: &'a EssIr,
    pub(crate) input: &'a Map<String, Value>,
    /// The attributes of the caller the command is sent as; `None` for no caller.
    pub(crate) caller: Option<&'a Map<String, Value>>,
    pub(crate) before: Option<&'a Row>,
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
