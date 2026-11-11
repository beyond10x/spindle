//! Computed values (ess/27, G1): `{expr: "<expression>"}` in `sets:` and `payload:` is computed
//! over the input and the subject as it was before the outcome, as ESS's interpreter computes it
//! (`interpret::execute::determined::expression`, over `ess_primitives::expr::Expr::evaluate`).
//!
//! The fixture `tests/fixtures/expressions` opens a ticket at an attempt the caller sends and
//! retries it one attempt higher (`subject.attempt + 1`); both set the expiry a week after the
//! request (`input.requested_at + 168h`), a renewal time after an `Optional` duration that falls
//! back to a week, and the share one part is of a whole, rounded half-even to one place and `0`
//! where the whole is zero.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, execute};

use super::related_guards::{Answer, interpreted, object, spindled};

const TICKET: &str = "relay.ticket.Ticket";
const OPEN: &str = "relay.ticket.OpenTicket";
const RETRY: &str = "relay.ticket.RetryTicket";

const FIRST: &str = "3d4e5f60-0000-4000-8000-000000000001";
const SECOND: &str = "3d4e5f60-0000-4000-8000-000000000002";
const THIRD: &str = "3d4e5f60-0000-4000-8000-000000000003";
const FOURTH: &str = "3d4e5f60-0000-4000-8000-000000000004";

const VIEWS: [(&str, &str, &str); 1] = [("relay.ticket.TicketCard", TICKET, "ticket_id")];

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/expressions"))
        .expect("the expressions fixture compiles")
        .ir
}

fn open(
    ticket: &str,
    attempt: i64,
    requested_at: &str,
    part: i64,
    whole: i64,
) -> (&'static str, Value) {
    (
        OPEN,
        json!({"ticket_id": ticket, "attempt": attempt, "requested_at": requested_at,
               "part": part, "whole": whole}),
    )
}

fn retry(ticket: &str, requested_at: &str, part: i64, whole: i64) -> (&'static str, Value) {
    (
        RETRY,
        json!({"ticket_id": ticket, "requested_at": requested_at, "part": part, "whole": whole}),
    )
}

fn with_ttl((command, mut input): (&'static str, Value), ttl: &str) -> (&'static str, Value) {
    input["ttl"] = json!(ttl);
    (command, input)
}

#[test]
fn every_outcome_with_a_computed_value_runs() {
    let ir = ir();
    for command in ir.commands().values() {
        spindle_kernel::runs(&ir, command)
            .unwrap_or_else(|error| panic!("`{}` runs every outcome: {error}", command.name));
    }
}

#[test]
fn computed_values_are_written_and_published() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let run = |store: &mut MemoryStore, (command, input): (&str, Value)| {
        execute(
            &ir,
            store,
            &mut NoExternals,
            command,
            &object(input.clone()),
        )
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
    };

    let opened = run(&mut store, open(FIRST, 4, "2026-11-01T09:30:00Z", 1, 16));
    assert_eq!(opened.outcome, "opened");
    assert_eq!(
        opened.events[0].payload["expires_at"],
        json!("2026-11-08T09:30:00Z")
    );
    assert_eq!(
        opened.events[0].payload["share"],
        json!(6.2),
        "6.25 rounds half-even to 6.2"
    );
    let retried = run(&mut store, retry(FIRST, "2026-12-28T23:00:00Z", 1, 0));
    assert_eq!(retried.outcome, "retried");
    assert_eq!(retried.events[0].payload["attempt"], json!(5));
    let row = store
        .load(TICKET, &json!(FIRST))
        .expect("the store reads")
        .expect("the ticket is held")
        .row;
    assert_eq!(row.fields["attempt"], json!(5), "subject.attempt + 1");
    assert_eq!(row.fields["expires_at"], json!("2027-01-04T23:00:00Z"));
    assert_eq!(row.fields["renew_by"], json!("2027-01-04T23:00:00Z"));
    assert_eq!(
        row.fields["share"],
        json!(0.0),
        "a zero whole falls back to 0, a Decimal written as ESS writes one"
    );
}

/// `value` as ESS's [`Node`], as a conformance target's answer is compared: the interpreter
/// writes an integral `Integer` as `2.0` and the kernel as `2`, one number either way.
fn node(value: Value) -> Node {
    serde_json::from_value(value.clone()).unwrap_or_else(|_| panic!("{value} is a node"))
}

fn nodes(fields: Map<String, Value>) -> BTreeMap<String, Node> {
    fields
        .into_iter()
        .map(|(field, value)| (field, node(value)))
        .collect()
}

type NodeAnswer = (
    String,
    Option<String>,
    Vec<(String, BTreeMap<String, Node>)>,
);

fn node_answer((outcome, error, events): Answer) -> NodeAnswer {
    let events = events
        .into_iter()
        .map(|(name, payload)| (name, nodes(payload)))
        .collect();
    (outcome, error, events)
}

fn node_rows(rows: Vec<Vec<Map<String, Value>>>) -> Vec<Vec<BTreeMap<String, Node>>> {
    rows.into_iter()
        .map(|view| view.into_iter().map(nodes).collect())
        .collect()
}

/// Steps every one of which ESS's interpreter answers: ties rounded half-even at the kept place,
/// a negative share, a zero whole, an expiry across a year and a leap day, an offset request time,
/// and a sent duration beside its fallback.
fn agreeing_steps() -> Vec<(&'static str, Value)> {
    vec![
        open(FIRST, 0, "2026-11-01T09:30:00Z", 1, 3),
        with_ttl(
            open(SECOND, 5, "2028-02-25T00:00:00.250Z", 1, 16),
            "PT1H30M",
        ),
        open(THIRD, -2, "2026-11-01T09:30:00+02:00", -3, 16),
        open(FOURTH, 9, "9999-12-24T23:59:59Z", 7, 0),
        with_ttl(retry(FIRST, "2026-12-28T23:00:00Z", 3, 16), "PT0S"),
        retry(FIRST, "2027-01-01T00:00:00Z", 5, 8),
        retry(SECOND, "2026-11-01T09:30:00Z", 1, 400),
        retry(SECOND, "2026-11-01T09:30:00Z", 1, -400),
        retry(THIRD, "2026-11-01T09:30:00Z", 2, 3),
    ]
}

#[test]
fn computed_values_agree_with_the_interpreter() {
    let ir = ir();
    let steps = agreeing_steps();
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let (answers, rows) = spindled(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "opened", "opened", "opened", "opened", "retried", "retried", "retried", "retried",
            "retried"
        ],
        "ESS answers every step"
    );
    assert_eq!(
        answers.into_iter().map(node_answer).collect::<Vec<_>>(),
        ess_answers.into_iter().map(node_answer).collect::<Vec<_>>(),
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        node_rows(rows),
        node_rows(ess_rows),
        "the stored tickets agree with ESS's view rows"
    );
}

/// What one side answers for a step: its answer, or that it determined no value.
type Decided = Result<Answer, String>;

fn answer_of_kernel(executed: spindle_kernel::Executed) -> Answer {
    (
        executed.outcome,
        executed.error.map(|error| error.name),
        executed
            .events
            .into_iter()
            .map(|event| (event.name, event.payload))
            .collect(),
    )
}

/// Each step as the kernel and as ESS's interpreter answer it, from one empty store each.
fn both(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Decided>, Vec<Decided>) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "relay.ticket/authored/expressions"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("expressions").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut store = MemoryStore::default();
    let (mut ess, mut kernel) = (Vec::new(), Vec::new());
    for (command, input) in steps {
        let nodes: BTreeMap<String, Node> = object(input.clone())
            .into_iter()
            .map(|(field, value)| {
                let node = serde_json::from_value(value).expect("a JSON scalar is a node");
                (field, node)
            })
            .collect();
        ess.push(
            target
                .execute_command(SemanticCommandRequest {
                    command: CommandRef::new(command.parse().expect("a command name")),
                    actor: None,
                    caller: None,
                    input: nodes,
                    correlation: context.correlation.clone(),
                })
                .map(|executed| {
                    (
                        executed
                            .outcome
                            .map(|outcome| outcome.outcome.as_str().to_owned())
                            .unwrap_or_default(),
                        executed.error.map(|error| error.error.to_string()),
                        executed
                            .direct_events
                            .iter()
                            .map(|event| {
                                (
                                    event.event.to_string(),
                                    event
                                        .payload
                                        .iter()
                                        .map(|(field, node)| {
                                            (
                                                field.clone(),
                                                serde_json::to_value(node).expect("JSON"),
                                            )
                                        })
                                        .collect::<Map<String, Value>>(),
                                )
                            })
                            .collect(),
                    )
                })
                .map_err(|error| error.to_string()),
        );
        kernel.push(
            match execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            ) {
                Ok(executed) => Ok(answer_of_kernel(executed)),
                Err(error @ ExecuteError::NoValue { .. }) => Err(error.to_string()),
                Err(other) => panic!("{command} {input}: the kernel answers {other:?}"),
            },
        );
    }
    (kernel, ess)
}

/// The bounds: an attempt one past the largest Integer, an expiry past the last instant RFC 3339
/// spells, a duration past every instant, and a share past the largest Integer a Decimal spells.
/// ESS determines no value for each and commits nothing; the kernel answers the same, never
/// wrapping, saturating or panicking.
#[test]
fn values_past_a_bound_are_undetermined_as_ess_decides() {
    let ir = ir();
    let steps = vec![
        open(FIRST, i64::MAX, "2026-11-01T09:30:00Z", 1, 3),
        retry(FIRST, "2026-11-01T09:30:00Z", 1, 3),
        open(SECOND, i64::MIN, "2026-11-01T09:30:00Z", 1, 3),
        retry(SECOND, "2026-11-01T09:30:00Z", 1, 3),
        open(THIRD, 0, "9999-12-25T00:00:00Z", 1, 3),
        with_ttl(
            open(THIRD, 0, "2026-11-01T09:30:00Z", 1, 3),
            "PT9223372036854775807S",
        ),
        open(THIRD, 0, "2026-11-01T09:30:00Z", i64::MAX, 1),
        open(THIRD, 0, "2026-11-01T09:30:00Z", i64::MIN, -1),
        open(THIRD, 0, "2026-11-01T09:30:00Z", i64::MAX, i64::MIN),
        open(FOURTH, 0, "0000-01-01T00:00:00Z", 1, 3),
    ];
    let (kernel, ess) = both(&ir, &steps);
    for (((command, input), kernel), ess) in steps.iter().zip(&kernel).zip(&ess) {
        match (kernel, ess) {
            (Ok(kernel), Ok(ess)) => assert_eq!(
                node_answer(kernel.clone()),
                node_answer(ess.clone()),
                "{command} {input}"
            ),
            (Err(_), Err(_)) => {}
            _ => panic!("{command} {input}: the kernel answers {kernel:?}, ESS {ess:?}"),
        }
    }
    let decided: Vec<bool> = kernel.iter().map(Result::is_ok).collect();
    assert_eq!(
        decided,
        [
            true, false, true, true, false, false, false, false, true, true
        ],
        "the steps past a bound are the ones undetermined: {kernel:?}"
    );
}
