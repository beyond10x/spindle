//! Adversary cases for `story:guard-over-absent-optional-input`: guards over optional inputs of
//! other shapes than the story's `Optional<Integer>`, each request run by the kernel and by ESS
//! 0.99.1's interpreter from an empty store, and the two answers compared.
//!
//! The fixture is written per test into a scratch directory under the cargo target directory.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, execute};

const SYSTEM: &str = "format: ess/35
system: probe
version: v1
summary: Probes of guards over optional inputs.
domains:
  - probe.item
components:
  - component: desk
    summary: Records items.
    owns:
      domains:
        - probe.item
    accepts:
      commands:
COMMANDS    publishes:
      events:
        - probe.item.ItemRecorded
    reached_by: network
";

const DOMAIN_HEAD: &str = "domain: probe.item
summary: Items, each recorded behind a guard over an optional input.
errors:
  - name: probe.item.Refused
    summary: The guard holds.
    fields: []
types:
  - name: probe.item.Kind
    kind: enum
    variants: [a, b]
  - name: probe.item.Box
    kind: struct
    fields:
      - name: w
        type: Optional<Integer>
entities:
  - name: probe.item.Item
    identity:
      name: id
      type: Uuid
    fields: []
    lifecycle:
      initial: Recorded
      states: [Recorded]
      terminal: [Recorded]
events:
  - name: probe.item.ItemRecorded
    fields:
      - name: id
        type: Uuid
commands:
";

/// One command `probe.item.<name>` with an `id` and the optional input `field: ty`, refused
/// where `guard` holds and recorded otherwise.
fn command(name: &str, field: &str, ty: &str, guard: &str) -> String {
    format!(
        "  - name: probe.item.{name}
    input:
      - name: id
        type: Uuid
      - name: {field}
        type: {ty}
    outcomes:
      - name: refused
        when: {guard}
        error: probe.item.Refused
        summary: The guard holds.
      - name: recorded
        creates: probe.item.Item
        instance: id
        emits: [probe.item.ItemRecorded]
        payload:
          probe.item.ItemRecorded:
            id: input.id
        summary: Recorded.
"
    )
}

fn ir(name: &str, field: &str, ty: &str, guard: &str) -> (tempfile::TempDir, spindle_spec::EssIr) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("domains");
    std::fs::write(
        dir.path().join("system.yaml"),
        SYSTEM.replace("COMMANDS", &format!("        - probe.item.{name}\n")),
    )
    .expect("system.yaml");
    std::fs::write(
        dir.path().join("domains/item.yaml"),
        format!("{DOMAIN_HEAD}{}", command(name, field, ty, guard)),
    )
    .expect("item.yaml");
    let ir = spindle_spec::load(Path::new(dir.path()))
        .unwrap_or_else(|error| panic!("the probe fixture compiles: {error}"))
        .ir;
    (dir, ir)
}

const ID: &str = "5e7a0000-0000-4000-8000-0000000000aa";

fn input(extra: &Value) -> Map<String, Value> {
    let mut input = Map::new();
    input.insert("id".to_owned(), json!(ID));
    for (field, value) in extra.as_object().expect("an object") {
        input.insert(field.clone(), value.clone());
    }
    input
}

/// The kernel's answer: the outcome, or the kind of its refusal.
fn kernel(ir: &spindle_spec::EssIr, command: &str, extra: &Value) -> String {
    match execute(
        ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        command,
        &input(extra),
    ) {
        Ok(executed) => executed.outcome,
        Err(error) => format!("refused {}: {error}", error.kind()),
    }
}

/// The interpreter's answer: the outcome, or `refused undecidable` for a guard it leaves
/// `Unknown`, or its refusal.
fn interpreter(ir: &spindle_spec::EssIr, command: &str, extra: &Value) -> String {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.item/authored/adversary-w25-guard"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w25-guard").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    match target.execute_command(SemanticCommandRequest {
        command: CommandRef::new(command.parse().expect("a command name")),
        actor: None,
        caller: None,
        input: input(extra)
            .into_iter()
            .map(|(field, value)| (field, serde_json::from_value(value).expect("a node")))
            .collect(),
        correlation: context.correlation.clone(),
    }) {
        Ok(executed) => executed
            .outcome
            .map(|outcome| outcome.outcome.as_str().to_owned())
            .unwrap_or_default(),
        Err(error) => {
            let text = error.to_string();
            if text.contains("is not what the model declares") {
                format!("refused invalid_input: {text}")
            } else if text.contains("is Unknown") {
                format!("refused undecidable: {text}")
            } else {
                format!("refused: {text}")
            }
        }
    }
}

/// The head of an answer: the outcome, or `refused <kind>`.
fn head(answer: &str) -> &str {
    answer.split(':').next().unwrap_or(answer)
}

/// Runs every request through both and fails on the first whose answers differ.
fn agree(name: &str, field: &str, ty: &str, guard: &str, requests: &[Value]) {
    let (_dir, ir) = ir(name, field, ty, guard);
    let command = format!("probe.item.{name}");
    let mut differ = Vec::new();
    for request in requests {
        let ours = kernel(&ir, &command, request);
        let theirs = interpreter(&ir, &command, request);
        if head(&ours) != head(&theirs) {
            differ.push(format!(
                "{field}: {ty}, `{guard}`, {request}: kernel `{ours}`, interpreter `{theirs}`"
            ));
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}

#[test]
fn a_string_guard_answers_as_the_interpreter() {
    agree(
        "StringEq",
        "note",
        "Optional<String>",
        "note == \"x\"",
        &[
            json!({}),
            json!({"note": null}),
            json!({"note": ""}),
            json!({"note": "x"}),
        ],
    );
}

#[test]
fn an_enum_guard_answers_as_the_interpreter() {
    agree(
        "EnumEq",
        "kind",
        "Optional<probe.item.Kind>",
        "kind == a",
        &[
            json!({}),
            json!({"kind": null}),
            json!({"kind": "a"}),
            json!({"kind": "b"}),
        ],
    );
}

#[test]
fn a_struct_member_guard_answers_as_the_interpreter() {
    agree(
        "BoxMember",
        "box",
        "Optional<probe.item.Box>",
        "box.w > 1",
        &[
            json!({}),
            json!({"box": null}),
            json!({"box": {}}),
            json!({"box": {"w": null}}),
            json!({"box": {"w": 2}}),
        ],
    );
}

#[test]
fn a_list_guard_answers_as_the_interpreter() {
    agree(
        "ListCount",
        "tags",
        "Optional<List<String>>",
        "tags.count > 0",
        &[
            json!({}),
            json!({"tags": null}),
            json!({"tags": []}),
            json!({"tags": ["x"]}),
        ],
    );
}

#[test]
fn a_coalesce_guard_answers_as_the_interpreter() {
    agree(
        "Coalesced",
        "weight",
        "Optional<Integer>",
        "coalesce(weight, 0) > 10",
        &[
            json!({}),
            json!({"weight": null}),
            json!({"weight": 20}),
            json!({"weight": 5}),
        ],
    );
}

#[test]
fn an_any_guard_answers_as_the_interpreter() {
    agree(
        "AnyOf",
        "weight",
        "Optional<Integer>",
        "{any: [weight > 10, defined(id)]}",
        &[json!({}), json!({"weight": null}), json!({"weight": 5})],
    );
}

#[test]
fn a_decimal_guard_answers_as_the_interpreter() {
    agree(
        "DecimalGt",
        "price",
        "Optional<Decimal>",
        "price > 10",
        &[
            json!({}),
            json!({"price": null}),
            json!({"price": 12.5}),
            json!({"price": 2.5}),
        ],
    );
}

#[test]
fn an_exists_over_optional_members_answers_as_the_interpreter() {
    agree(
        "BoxesExist",
        "boxes",
        "Optional<List<probe.item.Box>>",
        "{exists: {in: boxes, as: b, that: b.w > 1}}",
        &[
            json!({}),
            json!({"boxes": []}),
            json!({"boxes": [{}]}),
            json!({"boxes": [{}, {"w": 2}]}),
            json!({"boxes": [{"w": 0}]}),
        ],
    );
}

#[test]
fn a_defined_json_guard_answers_as_the_interpreter() {
    agree(
        "JsonDefined",
        "blob",
        "Optional<Json>",
        "defined(blob)",
        &[
            json!({}),
            json!({"blob": null}),
            json!({"blob": 5}),
            json!({"blob": {}}),
        ],
    );
}
