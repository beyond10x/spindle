//! Adversary of wave 27 U4 (`story:map-input-guard-facts`): a `Map` in the positions a guard
//! reads one, each request answered by the kernel and by ESS 0.99.1's interpreter.
//!
//! The fixture `tests/fixtures/adversary_w27_u4` reads a map of `Timestamp`s in a quantifier (and
//! a list of them, the control), a map inside a struct, a list of maps, a map of maps, a newtype
//! over an optional map, a stored map read by `when_subject:` and an input map read there under
//! `input.`. ESS orders a map's value at an ordinal by its declared type
//! (`input::declared_primitive` through `map_value`): a `Timestamp` by the instant it names. A
//! caller attribute that is a map, and an ordering over a `Duration`, are refused by the compiler,
//! so neither is reached.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, execute};

const STAMP_MAP: &str = "probe.ledger.StampMap";
const STAMP_LIST: &str = "probe.ledger.StampList";
const NESTED: &str = "probe.ledger.Nested";
const ROUNDS: &str = "probe.ledger.Rounds";
const GROUPS: &str = "probe.ledger.Groups";
const WRAPPED: &str = "probe.ledger.Wrapped";
const OPEN: &str = "probe.ledger.OpenSheet";
const CLOSE: &str = "probe.ledger.CloseSheet";

const IDS: [&str; 4] = [
    "7d1e0000-0000-4000-8000-000000000001",
    "7d1e0000-0000-4000-8000-000000000002",
    "7d1e0000-0000-4000-8000-000000000003",
    "7d1e0000-0000-4000-8000-000000000004",
];

/// One request: the command and its input.
struct Step {
    command: &'static str,
    input: Value,
}

fn step(command: &'static str, input: Value) -> Step {
    Step { command, input }
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary_w27_u4"),
    )
    .unwrap_or_else(|error| panic!("the adversary_w27_u4 fixture compiles: {error}"))
    .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn nodes(value: &Value) -> BTreeMap<String, Node> {
    object(value.clone())
        .into_iter()
        .map(|(name, value)| (name, serde_json::from_value(value).expect("a node")))
        .collect()
}

/// What the kernel answers for each step, on one store.
fn spindled(ir: &spindle_spec::EssIr, steps: &[Step]) -> Vec<Result<String, ExecuteError>> {
    let mut store = MemoryStore::default();
    steps
        .iter()
        .map(|step| {
            execute(
                ir,
                &mut store,
                &mut NoExternals,
                step.command,
                &object(step.input.clone()),
            )
            .map(|executed| executed.outcome)
        })
        .collect()
}

/// What ESS's interpreter answers for each step, in one scenario.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "probe.ledger/authored/adversary-w27-u4"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w27-u4").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|step| {
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(step.command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: nodes(&step.input),
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

/// The kernel's answers, as outcome names or refusal kinds, after checking each against the
/// interpreter's: the same outcome, or an undecidable refusal where the interpreter leaves the
/// same guard `Unknown`.
fn answered_as_the_interpreter(steps: &[Step]) -> Vec<String> {
    let ir = ir();
    let ours = spindled(&ir, steps);
    let theirs = interpreted(&ir, steps);
    for (index, (step, (ours, theirs))) in steps.iter().zip(ours.iter().zip(&theirs)).enumerate() {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => assert_eq!(
                ours, theirs,
                "request {index}: {} {}: the kernel selects `{ours}`, the interpreter `{theirs}`",
                step.command, step.input
            ),
            (Err(ExecuteError::Undecidable { outcome, .. }), Err(theirs))
                if theirs.contains(&format!("`{outcome}` is Unknown")) => {}
            _ => panic!(
                "request {index}: {} {}: the kernel answers {ours:?}, the interpreter {theirs:?}",
                step.command, step.input
            ),
        }
    }
    ours.iter()
        .map(|answer| match answer {
            Ok(outcome) => outcome.clone(),
            Err(refused) => refused.kind().to_owned(),
        })
        .collect()
}

/// A map of `Timestamp`s is ordered by the instant each value names at its ordinal, as ESS's
/// `declared_primitive` resolves `stamps.<index>` through the map's value type.
/// `2020-01-01T00:30:00+01:00` is 23:30 UTC the day before (before the cut-off, its bytes after);
/// `2019-12-31T23:30:00-01:00` is 00:30 UTC (after the cut-off, its bytes before).
#[test]
fn a_timestamp_map_value_is_ordered_by_its_instant() {
    let answers = answered_as_the_interpreter(&[
        step(
            STAMP_MAP,
            json!({"id": IDS[0], "stamps": {"a": "2020-01-01T00:30:00+01:00"}}),
        ),
        step(
            STAMP_MAP,
            json!({"id": IDS[1], "stamps": {"a": "2019-12-31T23:30:00-01:00"}}),
        ),
    ]);
    assert_eq!(answers, ["early", "opened"]);
}

/// The same guard over a list of `Timestamp`s: the control, where the path resolves.
#[test]
fn a_timestamp_list_value_is_ordered_by_its_instant() {
    let answers = answered_as_the_interpreter(&[
        step(
            STAMP_LIST,
            json!({"id": IDS[0], "stamps": ["2020-01-01T00:30:00+01:00"]}),
        ),
        step(
            STAMP_LIST,
            json!({"id": IDS[1], "stamps": ["2019-12-31T23:30:00-01:00"]}),
        ),
    ]);
    assert_eq!(answers, ["early", "opened"]);
}

#[test]
fn a_map_inside_a_struct_answers_as_the_interpreter() {
    let answers = answered_as_the_interpreter(&[
        step(
            NESTED,
            json!({"id": IDS[0], "card": {"label": "x", "scores": {}}}),
        ),
        step(
            NESTED,
            json!({"id": IDS[1], "card": {"label": "x", "scores": {"b": 1, "a": 2}}}),
        ),
        step(
            NESTED,
            json!({"id": IDS[2], "card": {"label": "x", "scores": {"a": 9}}}),
        ),
    ]);
    assert_eq!(answers, ["opened", "crowded", "high"]);
}

#[test]
fn a_list_of_maps_answers_as_the_interpreter() {
    let answers = answered_as_the_interpreter(&[
        step(ROUNDS, json!({"id": IDS[0], "rounds": [{"a": 1}, {}]})),
        step(
            ROUNDS,
            json!({"id": IDS[1], "rounds": [{"a": 1}, {"a": 1, "b": 2}]}),
        ),
    ]);
    assert_eq!(answers, ["opened", "crowded"]);
}

#[test]
fn a_map_of_maps_answers_as_the_interpreter() {
    let answers = answered_as_the_interpreter(&[
        step(
            GROUPS,
            json!({"id": IDS[0], "groups": {"x": {"a": 1}, "y": {}}}),
        ),
        step(
            GROUPS,
            json!({"id": IDS[1], "groups": {"x": {"a": 1}, "y": {"a": 1, "b": 2}}}),
        ),
        step(
            GROUPS,
            json!({"id": IDS[2], "groups": {"x": {"a": 1}, "y": {"b": 9}}}),
        ),
    ]);
    assert_eq!(answers, ["opened", "crowded", "high"]);
}

#[test]
fn a_newtype_over_an_optional_map_answers_as_the_interpreter() {
    let answers = answered_as_the_interpreter(&[
        step(WRAPPED, json!({"id": IDS[0], "tally": {}})),
        step(WRAPPED, json!({"id": IDS[1], "tally": {"a": 1, "b": 2}})),
        step(WRAPPED, json!({"id": IDS[2]})),
        step(WRAPPED, json!({"id": IDS[3], "tally": null})),
    ]);
    assert_eq!(answers, ["opened", "crowded", "undecidable", "undecidable"]);
}

#[test]
fn a_stored_map_and_an_input_map_under_input_answer_as_the_interpreter() {
    let answers = answered_as_the_interpreter(&[
        step(OPEN, json!({"id": IDS[0], "tallies": {"a": 1, "b": 2}})),
        step(OPEN, json!({"id": IDS[1], "tallies": {"a": 1}})),
        step(OPEN, json!({"id": IDS[2]})),
        step(CLOSE, json!({"id": IDS[0], "extra": {}})),
        step(CLOSE, json!({"id": IDS[1], "extra": {"z": 9}})),
        step(CLOSE, json!({"id": IDS[1], "extra": {"z": 1}})),
        step(CLOSE, json!({"id": IDS[2], "extra": {}})),
    ]);
    assert_eq!(
        answers,
        [
            "opened",
            "opened",
            "opened",
            "crowded",
            "high",
            "closed",
            "undecidable"
        ]
    );
}
