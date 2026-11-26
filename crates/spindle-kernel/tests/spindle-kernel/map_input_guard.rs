//! An input guard over a present `Map` input (`story:map-input-guard-facts`).
//!
//! The fixture `tests/fixtures/map-input-guard` opens boards with a map of scores. `OpenBoard` is
//! refused `pre-scored` where `when: scores.count > 0` holds over an `Optional<Map<String,
//! Integer>>`; `OpenLowBoard` is refused `high-score` where `exists: {in: scores, as: s, that: s >
//! 5}` holds; `OpenMixedBoard` is refused `all-high` where the same `forall` holds. ESS 0.99.1's
//! interpreter binds a map as it binds a list, `<path>.count` and its values at `<path>.<index>` in
//! key order, so each guard over a present map is decided. The kernel answers each request as the
//! interpreter does.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, execute};

use super::related_guards::object;

const BOARD: &str = "tally.board.Board";
const COUNTED: &str = "tally.board.OpenBoard";
const LOW: &str = "tally.board.OpenLowBoard";
const MIXED: &str = "tally.board.OpenMixedBoard";

const FIRST: &str = "5e7a0000-0000-4000-8000-00000000000a";
const SECOND: &str = "5e7a0000-0000-4000-8000-00000000000b";
const THIRD: &str = "5e7a0000-0000-4000-8000-00000000000c";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-input-guard"),
    )
    .unwrap_or_else(|error| panic!("the map-input-guard fixture compiles: {error}"))
    .ir
}

/// What the kernel answers for each request on `store`: the outcome, or the refusal.
fn spindled(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    requests: &[(&str, Value)],
) -> Vec<Result<String, ExecuteError>> {
    requests
        .iter()
        .map(|(command, input)| {
            execute(ir, store, &mut NoExternals, command, &object(input.clone()))
                .map(|executed| executed.outcome)
        })
        .collect()
}

/// What ESS's interpreter answers for each request, in one scenario: the outcome, or its refusal.
fn interpreted(
    ir: &spindle_spec::EssIr,
    requests: &[(&str, Value)],
) -> Vec<Result<String, String>> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "tally.board/authored/map-input-guard"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("map-input-guard").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    requests
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

/// The kernel's answers to `requests` on an empty store, after checking each against the
/// interpreter's: the same outcome, or an undecidable refusal where the interpreter leaves the
/// same guard `Unknown`.
fn answered_as_the_interpreter(
    ir: &spindle_spec::EssIr,
    store: &mut MemoryStore,
    requests: &[(&str, Value)],
) -> Vec<Result<String, ExecuteError>> {
    let ours = spindled(ir, store, requests);
    let theirs = interpreted(ir, requests);
    assert_eq!(ours.len(), theirs.len());
    for (index, ((command, input), (ours, theirs))) in
        requests.iter().zip(ours.iter().zip(&theirs)).enumerate()
    {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => {
                assert_eq!(ours, theirs, "request {index}: {command} {input}");
            }
            (Err(ExecuteError::Undecidable { outcome, .. }), Err(theirs)) => {
                assert!(
                    theirs.contains(&format!("`{outcome}` is Unknown")),
                    "request {index}: {command} {input}: the interpreter refuses the same \
                     guard: {theirs}"
                );
            }
            _ => panic!(
                "request {index}: {command} {input}: the kernel answers {ours:?}, the \
                 interpreter {theirs:?}"
            ),
        }
    }
    ours
}

/// The outcome of a decided answer, or the refusal's kind.
fn outcomes(answers: &[Result<String, ExecuteError>]) -> Vec<String> {
    answers
        .iter()
        .map(|answer| match answer {
            Ok(outcome) => outcome.clone(),
            Err(refused) => refused.kind().to_owned(),
        })
        .collect()
}

/// Whether the board `id` is held in `store`.
fn held(store: &mut MemoryStore, id: &str) -> bool {
    store
        .load(BOARD, &json!(id))
        .expect("the store reads")
        .is_some()
}

#[test]
fn a_map_count_guard_answers_as_the_interpreter() {
    let ir = ir();
    let requests = [
        (
            COUNTED,
            json!({"id": FIRST, "label": "empty", "scores": {}}),
        ),
        (
            COUNTED,
            json!({"id": SECOND, "label": "scored", "scores": {"a": 7}}),
        ),
        (COUNTED, json!({"id": THIRD, "label": "unscored"})),
    ];
    let mut store = MemoryStore::default();
    let ours = answered_as_the_interpreter(&ir, &mut store, &requests);
    assert_eq!(
        outcomes(&ours),
        ["opened", "pre-scored", "undecidable"],
        "an empty map counts zero, a map of one counts one, and an absent map is open"
    );
    assert!(held(&mut store, FIRST), "the empty board is opened");
    assert!(
        !held(&mut store, SECOND) && !held(&mut store, THIRD),
        "neither the refused nor the undecided board is written"
    );
}

#[test]
fn a_map_exists_guard_answers_as_the_interpreter() {
    let ir = ir();
    let requests = [
        (
            LOW,
            json!({"id": FIRST, "label": "low", "scores": {"a": 1}}),
        ),
        (
            LOW,
            json!({"id": SECOND, "label": "mixed", "scores": {"b": 9, "a": 1}}),
        ),
    ];
    let mut store = MemoryStore::default();
    let ours = answered_as_the_interpreter(&ir, &mut store, &requests);
    assert_eq!(
        outcomes(&ours),
        ["opened", "high-score"],
        "a score over five among the map's values selects `high-score`"
    );
    assert!(held(&mut store, FIRST) && !held(&mut store, SECOND));
}

#[test]
fn a_map_all_guard_answers_as_the_interpreter() {
    let ir = ir();
    let requests = [
        (
            MIXED,
            json!({"id": FIRST, "label": "low", "scores": {"a": 1}}),
        ),
        (
            MIXED,
            json!({"id": SECOND, "label": "mixed", "scores": {"b": 9, "a": 1}}),
        ),
        (
            MIXED,
            json!({"id": THIRD, "label": "high", "scores": {"a": 7, "b": 9}}),
        ),
    ];
    let mut store = MemoryStore::default();
    let ours = answered_as_the_interpreter(&ir, &mut store, &requests);
    assert_eq!(
        outcomes(&ours),
        ["opened", "opened", "all-high"],
        "only a map whose every value is over five selects `all-high`"
    );
    assert!(held(&mut store, FIRST) && held(&mut store, SECOND) && !held(&mut store, THIRD));
}
