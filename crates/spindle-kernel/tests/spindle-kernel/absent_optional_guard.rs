//! An input guard over an `Optional` input the request leaves out
//! (`story:guard-over-absent-optional-input`).
//!
//! The fixture `tests/fixtures/absent-optional-guard` records entries with an optional `weight`.
//! `RecordEntry` is refused `too-heavy` where `when: weight > 10` holds; `RecordCheckedEntry`
//! guards the same refusal with `all: [defined(weight), weight > 10]`. An absent optional binds
//! no fact, so ESS 0.99.1's interpreter reads `weight > 10` as `Unknown` and refuses
//! `RecordEntry` as undecidable; `defined(weight)` is `False` without it, so the checked guard is
//! decided and `RecordCheckedEntry` records the entry. The kernel answers each request as the
//! interpreter does.

use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use serde_json::{Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, execute};

use super::related_guards::object;

const ENTRY: &str = "tally.entry.Entry";
const RECORD: &str = "tally.entry.RecordEntry";
const CHECKED: &str = "tally.entry.RecordCheckedEntry";

const HEAVY: &str = "5e7a0000-0000-4000-8000-000000000001";
const LIGHT: &str = "5e7a0000-0000-4000-8000-000000000002";
const UNWEIGHED: &str = "5e7a0000-0000-4000-8000-000000000003";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/absent-optional-guard"),
    )
    .unwrap_or_else(|error| panic!("the absent-optional-guard fixture compiles: {error}"))
    .ir
}

/// The three requests of each command: weighed over the limit, weighed under it, and without
/// the weight.
fn requests(command: &'static str) -> Vec<(&'static str, Value)> {
    vec![
        (
            command,
            json!({"id": HEAVY, "label": "heavy", "weight": 20}),
        ),
        (command, json!({"id": LIGHT, "label": "light", "weight": 5})),
        (command, json!({"id": UNWEIGHED, "label": "unweighed"})),
    ]
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
        "tally.entry/authored/absent-optional-guard"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("absent-optional-guard").expect("a correlation id"),
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

/// Whether the entry `id` is held in `store`.
fn held(store: &mut MemoryStore, id: &str) -> bool {
    store
        .load(ENTRY, &json!(id))
        .expect("the store reads")
        .is_some()
}

#[test]
fn a_guard_over_an_absent_optional_input_answers_as_the_interpreter() {
    let ir = ir();
    let requests = requests(RECORD);
    let mut store = MemoryStore::default();
    let ours = spindled(&ir, &mut store, &requests);
    let theirs = interpreted(&ir, &requests);
    assert_eq!(ours.len(), theirs.len());
    for (request, (ours, theirs)) in ours.iter().zip(&theirs).enumerate() {
        match (ours, theirs) {
            (Ok(ours), Ok(theirs)) => assert_eq!(ours, theirs, "request {request}"),
            (Err(refused), Err(theirs)) => {
                assert_eq!(
                    refused.kind(),
                    "undecidable",
                    "request {request}: {refused}"
                );
                assert!(
                    theirs.contains("`tally.entry.RecordEntry/too-heavy` is Unknown"),
                    "request {request}: the interpreter refuses the same guard: {theirs}"
                );
            }
            _ => {
                panic!("request {request}: the kernel answers {ours:?}, the interpreter {theirs:?}")
            }
        }
    }

    assert!(
        matches!(&ours[0], Ok(outcome) if outcome == "too-heavy"),
        "a weight over ten is refused: {ours:?}"
    );
    assert!(
        matches!(&ours[1], Ok(outcome) if outcome == "recorded"),
        "a weight under ten is recorded: {ours:?}"
    );
    let Err(refused) = &ours[2] else {
        panic!("without the weight the guard is open: {ours:?}")
    };
    assert!(
        matches!(
            refused,
            ExecuteError::Undecidable { outcome, .. } if outcome == "tally.entry.RecordEntry/too-heavy"
        ),
        "{refused:?}"
    );
    assert!(
        refused
            .to_string()
            .contains("the guard of `tally.entry.RecordEntry/too-heavy` is undecidable"),
        "{refused}"
    );
    assert!(held(&mut store, LIGHT), "the weighed entry is recorded");
    assert!(
        !held(&mut store, HEAVY) && !held(&mut store, UNWEIGHED),
        "neither the refused nor the undecided entry is written"
    );
}

#[test]
fn a_guard_with_defined_over_an_absent_input_selects_its_outcome() {
    let ir = ir();
    let requests = requests(CHECKED);
    let mut store = MemoryStore::default();
    let ours = spindled(&ir, &mut store, &requests)
        .into_iter()
        .map(|answer| answer.map_err(|error| error.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(
        ours,
        [
            Ok("too-heavy".to_owned()),
            Ok("recorded".to_owned()),
            Ok("recorded".to_owned()),
        ],
        "`defined(weight)` decides the guard without the weight"
    );
    assert_eq!(
        ours,
        interpreted(&ir, &requests),
        "the kernel and the interpreter select the same outcome for every request"
    );
    assert!(
        held(&mut store, UNWEIGHED),
        "the entry without a weight is recorded"
    );
}
