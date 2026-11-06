//! Adversary cases for policies chosen per refusal: the in-process host against ESS 0.86.0's
//! interpreter, with failures that carry no declared outcome (`PortAnswer::Untyped` on the ESS
//! side, a decider with no answer on the host side), with a policy that changes on the last
//! attempt, and with a second fixture whose fallback is `drop` and whose named refusal retries.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef, ViewRef};
use ess_conformance::interpret::{Interpreted, PortAnswer};
use ess_conformance::{
    ConformanceTarget, Deadline, InvocationObservationRequest, ScenarioContext,
    SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, SystemClock};
use spindle_kernel::External;

const CLOSE: &str = "library.lending.CloseLoan";
const THANK: &str = "library.notices.SendThanks";
const BINDING: &str = "thank-on-close";
const VIEWS: [&str; 2] = ["library.lending.Loans", "library.notices.AllThanks"];

/// `drop: [no-reader]`, `retry: {except: [no-reader]}`: the fallback retries.
const RETRYING: &str = "crates/spindle-host/tests/fixtures/refusal";
/// `retry: [busy]`, `drop: {except: [busy]}`: the fallback drops.
const DROPPING: &str = "crates/spindle-host/tests/fixtures/adversary_refusal";

/// One attempt's answer: a declared refusal, or `None` for a failure with no declared outcome.
type Answer = Option<&'static str>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model(fixture: &str) -> EssIr {
    spindle_spec::load(&root().join(fixture))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

#[derive(Debug, PartialEq)]
struct Observed {
    attempts: usize,
    rows: BTreeMap<String, Vec<Value>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn interpreted(fixture: &str, script: &[Answer]) -> Observed {
    let target = Interpreted::for_model(model(fixture));
    let context = ScenarioContext::new(
        "library.notices/authored/adversary-refusal"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-refusal").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let thank = CommandRef::new(THANK.parse().expect("a command name"));
    target.script_binding_port(
        &thank,
        script.iter().map(|answer| match answer {
            Some(outcome) => {
                PortAnswer::Outcome(OutcomeName::new(*outcome).expect("an outcome name"))
            }
            None => PortAnswer::Untyped,
        }),
    );
    let result = target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(CLOSE.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: BTreeMap::from([
                ("loan_id".to_owned(), Node::Text("loan-1".to_owned())),
                ("member_id".to_owned(), Node::Text("m-7".to_owned())),
            ]),
            correlation: context.correlation.clone(),
        })
        .expect("the interpreter closes the loan");
    assert!(result.error.is_none(), "{result:?}");
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let attempts = target
        .observe_invocations(InvocationObservationRequest {
            binding: BINDING.parse::<BindingRef>().expect("a binding"),
            command: thank,
            correlation: context.correlation.clone(),
            deadline,
        })
        .expect("the interpreter answers its invocations")
        .len();
    let rows = VIEWS
        .iter()
        .map(|view| {
            let rows = target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline,
                })
                .expect("the interpreter reads the view")
                .rows
                .iter()
                .map(|row| {
                    Value::Object(
                        row.iter()
                            .map(|(field, value)| (field.clone(), json_of(value)))
                            .collect(),
                    )
                })
                .collect();
            ((*view).to_owned(), rows)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    Observed { attempts, rows }
}

/// Answers each attempt with the next scripted answer: a refusal holds its cause; `None` gives no
/// answer at all (the command fails with no declared outcome); a script run out takes `sent`. An
/// attempt starts when an outcome is asked about a second time.
#[derive(Default)]
struct Script {
    answers: VecDeque<Answer>,
    current: Option<Option<Answer>>,
    asked: BTreeSet<String>,
}

fn decider(
    script: &[Answer],
) -> impl FnMut(&External<'_>) -> Result<bool, String> + Send + 'static {
    let state = Arc::new(Mutex::new(Script {
        answers: script.iter().copied().collect(),
        ..Script::default()
    }));
    move |question: &External<'_>| {
        assert_eq!(question.command, THANK, "the only external causes");
        let mut script = state.lock().unwrap_or_else(PoisonError::into_inner);
        if script.current.is_none() || script.asked.contains(question.outcome) {
            let next = script.answers.pop_front();
            script.current = Some(next);
            script.asked.clear();
        }
        script.asked.insert(question.outcome.to_owned());
        match script.current {
            Some(Some(None)) => Err("no answer".to_owned()),
            Some(Some(Some(outcome))) => Ok(outcome == question.outcome),
            _ => Ok(false),
        }
    }
}

fn hosted(fixture: &str, script: &[Answer]) -> Observed {
    let mut host = Host::deciding(
        model(fixture),
        &laptop(),
        Arc::new(SystemClock),
        Path::new(""),
        Box::new(decider(script)),
    )
    .expect("the laptop admits the library");
    let Value::Object(input) = json!({"loan_id": "loan-1", "member_id": "m-7"}) else {
        unreachable!("an object")
    };
    let report = host.submit(CLOSE, input).expect("the loan closes");
    let thanked = |cause: &Cause, command: &str| {
        command == THANK
            && *cause
                == Cause::Delivered {
                    binding: BINDING.to_owned(),
                }
    };
    let attempts = report
        .ran
        .iter()
        .filter(|ran| thanked(&ran.cause, &ran.command))
        .map(|ran| ran.attempts)
        .chain(
            report
                .refused
                .iter()
                .filter(|refused| thanked(&refused.cause, &refused.command))
                .map(|refused| refused.attempts),
        )
        .chain(
            report
                .failed
                .iter()
                .filter(|failed| thanked(&failed.cause, &failed.command))
                .map(|failed| failed.attempts),
        )
        .sum::<u32>();
    let rows = VIEWS
        .iter()
        .map(|view| {
            let rows = host
                .list(view)
                .expect("the host reads the view")
                .into_iter()
                .map(Value::Object)
                .collect();
            ((*view).to_owned(), rows)
        })
        .collect();
    host.stop();
    Observed {
        attempts: usize::try_from(attempts).expect("a count"),
        rows,
    }
}

fn agree(fixture: &str, script: &[Answer], attempts: usize) {
    let ess = interpreted(fixture, script);
    assert_eq!(
        ess.attempts, attempts,
        "the interpreter, {fixture} {script:?}: {ess:?}"
    );
    assert_eq!(hosted(fixture, script), ess, "{fixture} {script:?}");
}

#[test]
fn adversary_untyped_failures_take_the_retrying_fallback_as_the_interpreter_does() {
    agree(RETRYING, &[None], 2);
    agree(RETRYING, &[None, Some("busy"), None], 4);
    agree(RETRYING, &[Some("busy"), None, Some("no-reader")], 3);
}

#[test]
fn adversary_a_drop_on_the_fifth_attempt_agrees_with_the_interpreter() {
    agree(
        RETRYING,
        &[
            Some("busy"),
            Some("busy"),
            Some("busy"),
            Some("busy"),
            Some("no-reader"),
        ],
        5,
    );
}

#[test]
fn adversary_a_dropping_fallback_with_a_retried_refusal_agrees_with_the_interpreter() {
    agree(DROPPING, &[Some("busy"), Some("busy")], 3);
    agree(DROPPING, &[Some("busy"), Some("no-reader")], 2);
    agree(DROPPING, &[None], 1);
    agree(DROPPING, &[Some("busy"), None], 2);
    agree(DROPPING, &[Some("no-reader")], 1);
}
