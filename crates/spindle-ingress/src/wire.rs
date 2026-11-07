//! Values the kernel holds, written as ESS publishes them on the wire: each declared field under
//! its wire name ([`ess_gen::schema::wire_field_name`]) and each enum variant in its wire spelling
//! ([`ess_domain::types::EnumVariant::wire`]), at every depth. The kernel keys and spells values by
//! their declared names; only what this ingress writes is mapped.

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedBody, ResolvedField, ResolvedTypeRef};
use serde_json::{Map, Value};

/// `values`, members of a message whose fields are `fields`, keyed by each field's wire name and
/// each value written by its type ([`value`]). A member no field declares is kept as it is.
pub(crate) fn fields(
    ir: &EssIr,
    fields: &[ResolvedField],
    values: &Map<String, Value>,
) -> Map<String, Value> {
    values
        .iter()
        .map(
            |(name, held)| match fields.iter().find(|field| field.name == *name) {
                Some(field) => (
                    ess_gen::schema::wire_field_name(field).to_owned(),
                    value(ir, &field.type_ref, held),
                ),
                None => (name.clone(), held.clone()),
            },
        )
        .collect()
}

/// `held`, a value of `type_ref`, as written on the wire: a struct's members by wire name, an enum
/// variant in its wire spelling, through `Optional`, `List`, `Map` values, newtypes and a union's
/// payload. A value not of the shape its type has is kept as it is.
pub(crate) fn value(ir: &EssIr, type_ref: &ResolvedTypeRef, held: &Value) -> Value {
    match (type_ref, held) {
        (ResolvedTypeRef::Primitive { .. }, _)
        | (ResolvedTypeRef::Optional { .. }, Value::Null) => held.clone(),
        (ResolvedTypeRef::Optional { of }, _) => value(ir, of, held),
        (ResolvedTypeRef::List { of }, Value::Array(items)) => {
            Value::Array(items.iter().map(|item| value(ir, of, item)).collect())
        }
        (ResolvedTypeRef::Map { value: of, .. }, Value::Object(entries)) => Value::Object(
            entries
                .iter()
                .map(|(key, entry)| (key.clone(), value(ir, of, entry)))
                .collect(),
        ),
        (ResolvedTypeRef::Declared { name }, _) => body(ir, &ir.named_type(name).body, held),
        (ResolvedTypeRef::List { .. } | ResolvedTypeRef::Map { .. }, _) => held.clone(),
    }
}

fn body(ir: &EssIr, declared: &ResolvedBody, held: &Value) -> Value {
    match (declared, held) {
        (ResolvedBody::Newtype { of, .. }, _) => value(ir, of, held),
        (
            ResolvedBody::Struct {
                fields: declared, ..
            },
            Value::Object(members),
        ) => Value::Object(fields(ir, declared, members)),
        (ResolvedBody::Enum { variants }, Value::String(text)) => variants
            .iter()
            .find(|variant| variant.name() == text)
            .map_or_else(
                || held.clone(),
                |variant| Value::String(variant.wire().to_owned()),
            ),
        (ResolvedBody::Union { tag, variants }, Value::Object(members)) => {
            let content = if tag == "value" { "content" } else { "value" };
            let payload = members
                .get(tag)
                .and_then(Value::as_str)
                .and_then(|label| variants.get(label))
                .and_then(Option::as_ref);
            Value::Object(
                members
                    .iter()
                    .map(|(key, member)| match payload {
                        Some(payload) if key == content => {
                            (key.clone(), value(ir, payload, member))
                        }
                        _ => (key.clone(), member.clone()),
                    })
                    .collect(),
            )
        }
        (
            ResolvedBody::Struct { .. } | ResolvedBody::Enum { .. } | ResolvedBody::Union { .. },
            _,
        ) => held.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::fields;
    use ess_compiler::ir::ResolvedBody;
    use serde_json::json;

    const SYSTEM: &str = "format: ess/34
system: shelf
version: v1
summary: A crate holding a tag and labels.
domains: [shelf.crate]
";

    const DOMAIN: &str = "domain: shelf.crate
summary: Crates.
types:
  - name: shelf.crate.Kind
    kind: enum
    variants:
      - name: big
        wire: BIG
      - name: small
  - name: shelf.crate.Tag
    kind: struct
    fields:
      - name: tag_code
        type: String
        naming: {wire: tagCode}
      - name: kind
        type: shelf.crate.Kind
  - name: shelf.crate.Label
    kind: union
    tag: kind
    variants:
      tagged: shelf.crate.Tag
      blank: null
  - name: shelf.crate.Holder
    kind: struct
    fields:
      - name: held_tag
        type: shelf.crate.Tag
        naming: {wire: heldTag}
      - name: labels
        type: List<shelf.crate.Label>
      - name: spare
        type: Optional<shelf.crate.Kind>
";

    #[test]
    fn every_declared_name_is_written_in_its_wire_form_at_every_depth() {
        let ir = spindle_spec::compile(
            &["system.yaml".to_owned(), "domains/crate.yaml".to_owned()],
            &[SYSTEM.to_owned(), DOMAIN.to_owned()],
        )
        .expect("the model compiles");
        let holder = ir
            .types()
            .iter()
            .find(|(name, _)| name.to_string() == "shelf.crate.Holder")
            .map(|(_, declared)| &declared.body);
        let Some(ResolvedBody::Struct {
            fields: declared, ..
        }) = holder
        else {
            panic!("the model declares the struct Holder");
        };
        let held = json!({
            "held_tag": {"tag_code": "t", "kind": "big"},
            "labels": [
                {"kind": "tagged", "value": {"tag_code": "u", "kind": "small"}},
                {"kind": "blank"}
            ],
            "spare": "big"
        });
        let held = held.as_object().expect("an object");
        assert_eq!(
            serde_json::Value::Object(fields(&ir, declared, held)),
            json!({
                "heldTag": {"tagCode": "t", "kind": "BIG"},
                "labels": [
                    {"kind": "tagged", "value": {"tagCode": "u", "kind": "small"}},
                    {"kind": "blank"}
                ],
                "spare": "BIG"
            })
        );
        let spare = json!({"spare": null});
        assert_eq!(
            serde_json::Value::Object(fields(&ir, declared, spare.as_object().expect("an object"))),
            json!({"spare": null}),
            "an absent optional stays null"
        );
    }
}
