//! Adversary cases for computed values (`{expr: …}`, `story:kernel-value-expressions`): odd
//! inputs the story's fixture does not send, each run by the kernel and by ESS 0.92.0's
//! interpreter from one empty store each, compared step by step and row by row.
//!
//! The fixture `tests/fixtures/adversary-expressions` sets a meter from an Integer, a Decimal, a
//! Timestamp plus a Duration, a chain of `coalesce`, a member of an `Optional` struct input and a
//! negative quotient rounded half-even, and steps it from the values it holds.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, Store, execute};

const METER: &str = "gauge.meters.Meter";
const SET: &str = "gauge.meters.SetMeter";
const STEP: &str = "gauge.meters.StepMeter";
const VIEW: &str = "gauge.meters.MeterCard";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-expressions"),
    )
    .expect("the adversary-expressions fixture compiles")
    .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn node(value: &Value) -> Node {
    serde_json::from_value(value.clone()).unwrap_or_else(|_| panic!("{value} is a node"))
}

/// What one side answers for a step: the outcome and each event's payload as nodes, or that it
/// ran no outcome.
type Decided = Result<(String, Vec<(String, BTreeMap<String, Node>)>), String>;

fn nodes(fields: &Map<String, Value>) -> BTreeMap<String, Node> {
    fields
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(field, value)| (field.clone(), node(value)))
        .collect()
}

/// One side's answer to every step, and the meters it holds after the last.
type Side = (Vec<Decided>, Vec<BTreeMap<String, Node>>);

/// Each step as the kernel and as ESS's interpreter answer it, and the meters each holds after
/// the last, from one empty store each.
fn both(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Side, Side) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "gauge.meters/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-expressions").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut store = MemoryStore::default();
    let (mut ess, mut kernel) = (Vec::new(), Vec::new());
    for (command, input) in steps {
        let nodes_in: BTreeMap<String, Node> = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node(value)))
            .collect();
        ess.push(
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: nodes_in,
                    correlation: context.correlation.clone(),
                })
                .map_err(|error| error.to_string())
                .and_then(|executed| {
                    if let Some(error) = executed.error {
                        return Err(format!("declared error {}", error.error));
                    }
                    // No outcome taken: the interpreter's answer to an identity no row carries
                    // where the command declares no branch for it.
                    let Some(outcome) = executed.outcome else {
                        return Err("no outcome".to_owned());
                    };
                    Ok((
                        outcome.outcome.as_str().to_owned(),
                        executed
                            .direct_events
                            .iter()
                            .map(|event| {
                                (
                                    event.event.to_string(),
                                    event
                                        .payload
                                        .iter()
                                        .filter(|(_, node)| **node != Node::Null)
                                        .map(|(field, node)| (field.clone(), node.clone()))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ))
                }),
        );
        kernel.push(
            execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            )
            .map_err(|error| error.to_string())
            .and_then(|executed| {
                if let Some(error) = executed.error {
                    return Err(format!("declared error {}", error.name));
                }
                Ok((
                    executed.outcome,
                    executed
                        .events
                        .iter()
                        .map(|event| (event.name.clone(), nodes(&event.payload)))
                        .collect(),
                ))
            }),
        );
    }
    let ess_rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(VIEW.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            row.iter()
                .filter(|(_, node)| **node != Node::Null)
                .map(|(field, node)| (field.clone(), node.clone()))
                .collect()
        })
        .collect();
    let kernel_rows = store
        .scan(METER)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("meter_id".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            nodes(&row)
        })
        .collect();
    ((kernel, kernel_rows), (ess, ess_rows))
}

fn set(meter: &str, changes: &Value) -> (&'static str, Value) {
    let mut input = json!({"meter_id": meter, "n": 5, "d": 0.1,
                           "at": "2026-11-01T09:30:00Z", "span": "PT1H"});
    for (field, value) in changes.as_object().expect("an object") {
        input[field] = value.clone();
    }
    (SET, input)
}

fn step(meter: &str, span: &str) -> (&'static str, Value) {
    (STEP, json!({"meter_id": meter, "span": span}))
}

/// Odd values a caller sends: an Integer written as a JSON float, Decimals binary64 does not
/// carry, a leap second, offsets and a lowercase `t`/`z`, durations in other ISO forms, a chain
/// of `coalesce` over absent and `null`, a member of an absent or present struct input, a
/// negative quotient rounded half-even on a tie, and an Integer whose product passes `i64` only
/// before the subtraction brings it back.
fn odd_steps() -> Vec<(&'static str, Value)> {
    let cases: Vec<Value> = vec![
        json!({}),
        json!({"n": 3.0}),
        json!({"n": -3}),
        json!({"n": 1}),
        json!({"n": -1}),
        json!({"n": 3_074_457_345_618_258_603_i64}),
        json!({"n": u64::MAX}),
        json!({"n": 1e20}),
        json!({"n": -0.0}),
        json!({"d": 1e300}),
        json!({"d": "0.10"}),
        json!({"d": 0.300_000_000_000_000_04}),
        json!({"d": 1e-7}),
        json!({"d": -0.0}),
        json!({"d": 12_345_678_901_234_567.0}),
        json!({"at": "2016-12-31T23:59:60Z"}),
        json!({"at": "2026-11-01T09:30:00-00:00"}),
        json!({"at": "2026-11-01T09:30:00+14:00"}),
        json!({"at": "2026-11-01t09:30:00z"}),
        json!({"at": "2026-11-01T09:30:00.123456789Z"}),
        json!({"at": "2026-11-01 09:30:00Z"}),
        json!({"span": "PT36H"}),
        json!({"span": "PT90M"}),
        json!({"span": "PT0S"}),
        json!({"span": "P1D"}),
        json!({"span": "PT1.5S"}),
        json!({"span": "PT1M1H"}),
        json!({"span": "-PT1H"}),
        json!({"span": "PT"}),
        json!({"span": "pt1h"}),
        json!({"a": 4}),
        json!({"a": null, "b": 6}),
        json!({"a": null, "b": null}),
        json!({"window": {"hours": 2}}),
        json!({"window": {"hours": 2, "extra": null}}),
        json!({"window": {"hours": 2, "extra": 3}}),
        json!({"window": {"hours": 2.0}}),
        json!({"window": null}),
    ];
    let mut steps = Vec::new();
    for (index, changes) in cases.iter().enumerate() {
        let meter = format!("m{index:02}");
        steps.push(set(&meter, changes));
        steps.push(step(&meter, "PT2H"));
    }
    steps
}

#[test]
fn odd_inputs_compute_as_the_interpreter_computes_them() {
    let ir = ir();
    let steps = odd_steps();
    let ((kernel, kernel_rows), (ess, ess_rows)) = both(&ir, &steps);
    let mut disagreeing = Vec::new();
    for (((command, input), kernel), ess) in steps.iter().zip(&kernel).zip(&ess) {
        let agree = match (kernel, ess) {
            (Ok(kernel), Ok(ess)) => kernel == ess,
            (Err(_), Err(_)) => true,
            _ => false,
        };
        if !agree {
            disagreeing.push(format!(
                "{command} {input}\n  kernel: {kernel:?}\n  ESS:    {ess:?}"
            ));
        }
    }
    assert!(
        disagreeing.is_empty(),
        "steps the kernel and ESS answer differently:\n{}",
        disagreeing.join("\n")
    );
    assert_eq!(
        kernel_rows, ess_rows,
        "the stored meters agree with ESS's view rows"
    );
}
