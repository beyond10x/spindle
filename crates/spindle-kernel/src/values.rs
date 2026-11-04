//! The value one determined field takes: an event's payload field, an error's field, or a field
//! `sets:` writes.
//!
//! Only the sources Spindle's own specification and the `rally` example use are read here; the
//! rest are refused by [`crate::supported`] before a command runs. Numbers are ESS's own
//! [`Number`], so `{increment: N}` adds exactly as ESS adds, and a literal is typed by
//! [`EssIr::literal_primitive`], the one question every ESS consumer asks of a literal.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedPayloadField, ResolvedPayloadValue};
use ess_domain::types::Primitive;
use ess_primitives::facts::Number;
use serde_json::{Map, Value};

use crate::ExecuteError;
use crate::store::Row;

/// What a determined field may read: the command's input and the addressed instance as it was
/// immediately before the outcome.
pub(crate) struct Sources<'a> {
    pub(crate) ir: &'a EssIr,
    pub(crate) input: &'a Map<String, Value>,
    pub(crate) before: Option<&'a Row>,
    /// Whether the fields are a declared error's: there, a field read from the instance is absent
    /// where no instance is held (the unknown-instance arm of a `wrong_state:` refusal, ESS
    /// `error_value`, beyond10x/ess#458) rather than a value nothing determines.
    pub(crate) error: bool,
    /// The identity this outcome minted for the instance it creates, under the identity field's
    /// name: what `{generated: true}` in a field of that name reads.
    pub(crate) minted: Option<(&'a str, &'a Value)>,
}

impl Sources<'_> {
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
            ResolvedPayloadValue::Generated { .. }
                if self
                    .minted
                    .is_some_and(|(identity, _)| identity == field.target) =>
            {
                Ok(self.minted.map(|(_, value)| value.clone()))
            }
            ResolvedPayloadValue::EmptyList => Ok(Some(Value::Array(Vec::new()))),
            ResolvedPayloadValue::Cleared => Ok(None),
            ResolvedPayloadValue::Struct { fields } => {
                let mut members = Map::new();
                for member in fields {
                    if let Some(value) = self.value(member)? {
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

    fn before(&self, field: &ResolvedPayloadField) -> Result<&Row, ExecuteError> {
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
