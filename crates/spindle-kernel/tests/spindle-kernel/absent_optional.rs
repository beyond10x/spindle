//! A row-set guard over an `Optional` field a stored row leaves absent
//! (`story:absent-optional-in-row-set-filter`).
//!
//! The fixture `tests/fixtures/absent-optional` keeps boxes, and items that sit in one box or in
//! none (`box_id: Optional<Uuid>`). `RetireBox` is refused `not-empty` where
//! `when_related: {entity: Item, where: [box_id == input.id], count: {gte: 1}}` holds. ESS reads
//! `box_id == input.id` as `Unknown` on an item whose `box_id` is absent, so that item is a
//! possible member and the count is open: ESS's interpreter refuses the command as undecidable,
//! and so does the kernel, naming the filter, the entity and the absent field, and the way out.
//! `tests/fixtures/absent-optional-defined` is the same model with that way out written,
//! `where: [defined(box_id), box_id == input.id]`, which both decide.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, Executed, MemoryStore, NoExternals, Store, execute};

use super::related_guards::object;

const BOX: &str = "store.shelf.Box";
const CREATE: &str = "store.shelf.CreateBox";
const ADD: &str = "store.shelf.AddItem";
const PUT: &str = "store.shelf.PutItemInBox";
const RETIRE: &str = "store.shelf.RetireBox";

const A: &str = "0b0c0d0e-0000-4000-8000-00000000000a";
const B: &str = "0b0c0d0e-0000-4000-8000-00000000000b";
const C: &str = "0b0c0d0e-0000-4000-8000-00000000000c";
const D: &str = "0b0c0d0e-0000-4000-8000-00000000000d";
const OTHER: &str = "0b0c0d0e-0000-4000-8000-0000000000ff";
const BOXED: &str = "e1e2e3e4-0000-4000-8000-000000000001";
const LOOSE: &str = "e1e2e3e4-0000-4000-8000-000000000002";
const INSIDE: &str = "e1e2e3e4-0000-4000-8000-000000000003";

fn ir(fixture: &str) -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture),
    )
    .unwrap_or_else(|error| panic!("the {fixture} fixture compiles: {error}"))
    .ir
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input))
}

fn run(ir: &spindle_spec::EssIr, store: &mut MemoryStore, command: &str, input: Value) -> String {
    send(ir, store, command, input.clone())
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
        .outcome
}

/// The steps of the story's cases: (a) an empty box beside no item, (b) one beside an item in
/// another box, (c) one beside an item in no box, and (d) a box holding an item.
fn steps() -> Vec<(&'static str, Value)> {
    vec![
        (CREATE, json!({"id": A, "label": "a"})),
        (RETIRE, json!({"id": A})),
        (CREATE, json!({"id": B, "label": "b"})),
        (CREATE, json!({"id": OTHER, "label": "other"})),
        (ADD, json!({"id": BOXED, "label": "i1"})),
        (PUT, json!({"id": BOXED, "box_id": OTHER})),
        (RETIRE, json!({"id": B})),
        (CREATE, json!({"id": C, "label": "c"})),
        (ADD, json!({"id": LOOSE, "label": "i2"})),
        (RETIRE, json!({"id": C})),
        (CREATE, json!({"id": D, "label": "d"})),
        (ADD, json!({"id": INSIDE, "label": "i3"})),
        (PUT, json!({"id": INSIDE, "box_id": D})),
        (RETIRE, json!({"id": D})),
    ]
}

/// What the kernel answers for each step: the outcome, or the refusal's kind.
fn spindled(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, String>> {
    let mut store = MemoryStore::default();
    steps
        .iter()
        .map(|(command, input)| {
            send(ir, &mut store, command, input.clone())
                .map(|executed| executed.outcome)
                .map_err(|error| error.kind().to_owned())
        })
        .collect()
}

/// What ESS's interpreter answers for each step: the outcome, or that it refused the step.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "store.shelf/authored/absent-optional"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("absent-optional").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|(command, input)| {
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: object(input.clone())
                        .iter()
                        .map(|(field, value)| {
                            (
                                field.clone(),
                                serde_json::from_value(value.clone()).expect("a node"),
                            )
                        })
                        .collect(),
                    correlation: context.correlation.clone(),
                })
                .map(|executed| {
                    executed
                        .outcome
                        .map(|outcome| outcome.outcome.as_str().to_owned())
                        .unwrap_or_default()
                })
                .map_err(|error| error.to_string())
        })
        .collect()
}

#[test]
fn a_row_set_guard_over_an_absent_optional_names_the_field() {
    let ir = ir("absent-optional");
    let mut store = MemoryStore::default();
    assert_eq!(
        run(&ir, &mut store, CREATE, json!({"id": C, "label": "c"})),
        "created"
    );
    assert_eq!(
        run(&ir, &mut store, ADD, json!({"id": LOOSE, "label": "i2"})),
        "added"
    );
    let refused = send(&ir, &mut store, RETIRE, json!({"id": C}))
        .expect_err("an item in no box leaves the count open");
    assert_eq!(refused.kind(), "undecidable", "{refused}");
    let message = refused.to_string();
    for named in [
        "`store.shelf.RetireBox/not-empty`",
        "`box_id == input.id`",
        "`store.shelf.Item`",
        "`box_id` is absent",
        "`defined(box_id)`",
    ] {
        assert!(message.contains(named), "{named} in: {message}");
    }
    let held = store
        .load(BOX, &json!(C))
        .expect("the store reads")
        .expect("the box is still held");
    assert_eq!(held.row.state, "Open", "nothing is written");
}

#[test]
fn a_row_set_guard_with_defined_over_an_absent_optional_is_decided() {
    let ir = ir("absent-optional-defined");
    let answers = spindled(&ir, &steps());
    let retired: Vec<_> = [1, 6, 9, 13].iter().map(|step| &answers[*step]).collect();
    assert_eq!(
        retired,
        [
            &Ok("retired".to_owned()),
            &Ok("retired".to_owned()),
            &Ok("retired".to_owned()),
            &Ok("not-empty".to_owned()),
        ],
        "(a), (b) and (c) retire the empty box; (d) is refused for its item: {answers:?}"
    );
}

#[test]
fn a_row_set_guard_over_an_absent_optional_agrees_with_the_interpreter() {
    let plain = ir("absent-optional");
    let ours = spindled(&plain, &steps());
    let theirs = interpreted(&plain, &steps());
    assert_eq!(ours.len(), theirs.len());
    for (step, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => assert_eq!(ours, theirs, "step {step}"),
            (Err(kind), Err(refused)) => {
                assert_eq!(kind, "undecidable", "step {step}");
                assert!(refused.contains("Unknown"), "step {step}: {refused}");
            }
            _ => panic!("step {step}: the kernel answers {ours:?}, the interpreter {theirs:?}"),
        }
    }
    assert!(
        matches!(&ours[9], Err(kind) if kind == "undecidable"),
        "case (c) is refused by both: {ours:?}"
    );
    let defined = ir("absent-optional-defined");
    assert_eq!(
        spindled(&defined, &steps()),
        interpreted(&defined, &steps()),
        "with `defined(box_id)` both decide every case alike"
    );
}
