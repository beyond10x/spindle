//! A `when_related:` guard over a `List` of references (ess/28,
//! `story:when-related-over-reference-list`): the kernel reads the row each element names and
//! takes the `exists: false` branch where any element names no row, an empty list naming none,
//! and `{missing: input.<list>}` is the first such element in list order, as ESS's interpreter at
//! the pinned tag decides and answers it.
//!
//! The fixture `tests/fixtures/reference-list` adds tools and assembles kits, each under a minted
//! identity, from a list of tools (`AssembleKit`); `ReviseKit` also asks a lock the specification
//! does not describe before the kit is read (`before_row: true`). Each step list runs on a fresh
//! store, on the kernel and on the interpreter, with the same causes forced.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, OutcomeRef, ViewRef};
use ess_conformance::ExternalOutcomeControl;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{External, MemoryStore, Store, execute};

const KIT: &str = "workshop.bench.Kit";
const KITS: &str = "workshop.bench.Kits";
const ADD_TOOL: &str = "workshop.bench.AddTool";
const ASSEMBLE_KIT: &str = "workshop.bench.AssembleKit";
const REVISE_KIT: &str = "workshop.bench.ReviseKit";
const UNKNOWN_TOOL: &str = "unknown-tool";
const LOCKED: &str = "locked";
const KIT_ASSEMBLED: &str = "workshop.bench.KitAssembled";
/// In a step's input, the kit the run assembled last; in an answer or a row, any kit it assembled.
const MINTED_KIT: &str = "<kit>";
/// The kit a step names before the run assembled one: no row carries it.
const NO_KIT: &str = "20000000-0000-4000-8000-00000000000a";
const TOOL_ONE: &str = "10000000-0000-4000-8000-000000000001";
const TOOL_TWO: &str = "10000000-0000-4000-8000-000000000002";
const NO_TOOL: &str = "10000000-0000-4000-8000-0000000000f1";
const NO_TOOL_EITHER: &str = "10000000-0000-4000-8000-0000000000f2";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference-list"))
        .expect("the reference-list fixture compiles")
        .ir
}

/// One step: the command, its input, and the outcome whose external cause is forced to hold.
type Step = (&'static str, Value, Option<&'static str>);

/// One step's answer: outcome, declared error and its fields, events by name and payload; `Err`
/// where no answer was given.
type Answer = Result<
    (
        String,
        Option<(String, Map<String, Value>)>,
        Vec<(String, Map<String, Value>)>,
    ),
    (),
>;

type Rows = Vec<Map<String, Value>>;

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON value is a node")
}

fn tools() -> Vec<Step> {
    [TOOL_ONE, TOOL_TWO]
        .into_iter()
        .map(|tool| (ADD_TOOL, json!({"tool_id": tool, "label": "t"}), None))
        .collect()
}

fn assemble(tools: &[&str]) -> Step {
    (ASSEMBLE_KIT, json!({"tools": tools}), None)
}

fn revise(tools: &[&str], forced: Option<&'static str>) -> Step {
    (
        REVISE_KIT,
        json!({"kit_id": MINTED_KIT, "tools": tools}),
        forced,
    )
}

/// The kit identities one run minted: a step's [`MINTED_KIT`] names the last, and each is
/// answered as [`MINTED_KIT`], since the kernel and the interpreter mint different values.
#[derive(Default)]
struct Minted(Vec<Value>);

impl Minted {
    /// `input`, its [`MINTED_KIT`] the last kit minted, or [`NO_KIT`] before any.
    fn input(&self, input: &Value) -> Map<String, Value> {
        let mut input = object(input.clone());
        if input.get("kit_id") == Some(&json!(MINTED_KIT)) {
            let kit = self.0.last().cloned().unwrap_or_else(|| json!(NO_KIT));
            input.insert("kit_id".to_owned(), kit);
        }
        input
    }

    /// Notes the kit `answer` minted, where it assembled one.
    fn note(&mut self, answer: &Answer) {
        let Ok((_, _, events)) = answer else {
            return;
        };
        for (event, payload) in events {
            if event == KIT_ASSEMBLED
                && let Some(kit) = payload.get("kit_id")
            {
                self.0.push(kit.clone());
            }
        }
    }

    /// `map`, each minted kit in it [`MINTED_KIT`].
    fn hide(&self, map: Map<String, Value>) -> Map<String, Value> {
        map.into_iter()
            .map(|(field, value)| {
                let value = if self.0.contains(&value) {
                    json!(MINTED_KIT)
                } else {
                    value
                };
                (field, value)
            })
            .collect()
    }

    /// `answer`, each minted kit in it [`MINTED_KIT`].
    fn answer(&self, answer: Answer) -> Answer {
        answer.map(|(outcome, error, events)| {
            (
                outcome,
                error.map(|(name, fields)| (name, self.hide(fields))),
                events
                    .into_iter()
                    .map(|(name, payload)| (name, self.hide(payload)))
                    .collect(),
            )
        })
    }
}

fn with_tools(steps: impl IntoIterator<Item = Step>) -> Vec<Step> {
    tools().into_iter().chain(steps).collect()
}

/// The step lists, each with what it shows.
fn cases() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        (
            "every referenced row present",
            with_tools([assemble(&[TOOL_ONE, TOOL_TWO])]),
        ),
        (
            "one referenced row missing, between two present",
            with_tools([assemble(&[TOOL_ONE, NO_TOOL, TOOL_TWO])]),
        ),
        (
            "two missing: the first in list order is named",
            with_tools([assemble(&[NO_TOOL_EITHER, TOOL_ONE, NO_TOOL])]),
        ),
        ("an empty list", vec![assemble(&[])]),
        ("no tool at all, one named", vec![assemble(&[NO_TOOL])]),
        (
            "a revision: present, missing, empty",
            with_tools([
                assemble(&[TOOL_ONE]),
                revise(&[TOOL_ONE, TOOL_TWO], None),
                revise(&[TOOL_TWO, NO_TOOL], None),
                revise(&[], None),
            ]),
        ),
        (
            "a revision of no kit, naming a missing tool",
            with_tools([revise(&[NO_TOOL], None)]),
        ),
        (
            "a locked revision answers before the kit and the tools are read",
            with_tools([
                revise(&[NO_TOOL], Some(LOCKED)),
                assemble(&[TOOL_ONE]),
                revise(&[NO_TOOL], Some(LOCKED)),
                revise(&[TOOL_TWO], Some(LOCKED)),
            ]),
        ),
    ]
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "workshop.bench/authored/reference-list"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("reference-list").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    let mut minted = Minted::default();
    for (command, input, forced) in steps {
        if let Some(outcome) = forced {
            target
                .configure_external_outcome(ExternalOutcomeControl {
                    force: OutcomeRef::new(
                        command.parse::<CommandRef>().expect("a command"),
                        OutcomeName::new(*outcome).expect("an outcome name"),
                    ),
                    correlation: context.correlation.clone(),
                })
                .expect("the interpreter takes the forcing");
        }
        let input = minted
            .input(input)
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target.execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input,
            correlation: context.correlation.clone(),
        });
        let answer: Answer = executed.map_err(|_| ()).map(|executed| {
            (
                executed
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_default(),
                executed.error.map(|error| {
                    (
                        error.error.to_string(),
                        error
                            .fields
                            .iter()
                            .map(|(field, node)| (field.clone(), json_of(node)))
                            .collect(),
                    )
                }),
                executed
                    .direct_events
                    .iter()
                    .map(|event| {
                        (
                            event.event.to_string(),
                            event
                                .payload
                                .iter()
                                .map(|(field, node)| (field.clone(), json_of(node)))
                                .collect(),
                        )
                    })
                    .collect(),
            )
        });
        minted.note(&answer);
        answers.push(answer);
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(KITS.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            minted.hide(
                row.iter()
                    .map(|(field, node)| (field.clone(), json_of(node)))
                    .collect(),
            )
        })
        .collect();
    let answers = answers
        .into_iter()
        .map(|answer| minted.answer(answer))
        .collect();
    (answers, rows)
}

fn spindled(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    let mut minted = Minted::default();
    for (command, input, forced) in steps {
        let input = minted.input(input);
        let mut decider = |question: &External<'_>| -> Result<bool, String> {
            Ok(Some(question.outcome) == *forced)
        };
        let before = store.events().len();
        let executed = execute(ir, &mut store, &mut decider, command, &input);
        if executed.is_err() {
            assert_eq!(
                store.events().len(),
                before,
                "{command} {input:?}: a step the kernel does not answer writes nothing"
            );
        }
        let answer: Answer = executed.map_err(|_| ()).map(|executed| {
            (
                executed.outcome,
                executed.error.map(|error| (error.name, error.fields)),
                executed
                    .events
                    .into_iter()
                    .map(|event| (event.name, event.payload))
                    .collect(),
            )
        });
        minted.note(&answer);
        answers.push(answer);
    }
    let rows = store
        .scan(KIT)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("kit_id".to_owned(), identity);
            minted.hide(row)
        })
        .collect();
    let answers = answers
        .into_iter()
        .map(|answer| minted.answer(answer))
        .collect();
    (answers, rows)
}

fn outcome(answer: &Answer) -> Option<&str> {
    answer.as_ref().ok().map(|(outcome, _, _)| outcome.as_str())
}

#[test]
fn every_command_over_a_list_of_references_runs() {
    let ir = ir();
    for (name, command) in ir.commands() {
        if let Err(refused) = spindle_kernel::runs(&ir, command) {
            panic!("the kernel runs every outcome of {name}: {refused}");
        }
    }
}

#[test]
fn a_guard_over_a_list_of_references_is_decided_as_the_interpreter() {
    let ir = ir();
    for (what, steps) in cases() {
        let (ess_answers, ess_rows) = interpreted(&ir, &steps);
        let (answers, rows) = spindled(&ir, &steps);
        for (step, ((command, input, forced), (ours, theirs))) in steps
            .iter()
            .zip(answers.iter().zip(&ess_answers))
            .enumerate()
        {
            assert_eq!(
                ours, theirs,
                "{what}: step {step} {command} {input} forced {forced:?}: the kernel's answer \
                 (left) is the interpreter's (right)"
            );
        }
        assert_eq!(rows, ess_rows, "{what}: the stored rows agree");
    }
}

#[test]
fn a_missing_reference_names_the_first_missing_element() {
    let ir = ir();
    let (answers, rows) = spindled(
        &ir,
        &with_tools([
            assemble(&[TOOL_ONE, NO_TOOL_EITHER, NO_TOOL]),
            assemble(&[]),
        ]),
    );
    let Ok((taken, Some((error, fields)), events)) = &answers[2] else {
        panic!("the refusal reports its error: {:?}", answers[2]);
    };
    assert_eq!(taken, UNKNOWN_TOOL);
    assert_eq!(error, "workshop.bench.UnknownTool");
    assert_eq!(fields, &object(json!({"tool_id": NO_TOOL_EITHER})));
    assert!(events.is_empty(), "a refusal emits nothing: {events:?}");
    assert_eq!(
        outcome(&answers[3]),
        Some("assembled"),
        "an empty list names no missing row"
    );
    assert_eq!(
        rows,
        vec![object(json!({"kit_id": MINTED_KIT, "tools": []}))],
        "only the empty kit is assembled"
    );
}
