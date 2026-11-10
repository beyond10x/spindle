//! A binding whose failure policy is chosen per refusal, run by the in-process host and by ESS
//! 0.87.0's interpreter on the same neutral fixture: closing a loan thanks its member, giving up
//! at once on `no-reader` and retrying any other refusal. For each sequence of answers the notices
//! give, both make the same attempts and leave the same rows.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef, ViewRef};
use ess_conformance::interpret::{Interpreted, PortAnswer};
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, Deadline, InvocationObservationRequest, ScenarioContext,
    SemanticCommandRequest, SemanticViewRequest, Status,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_host::{Cause, Host, SystemClock};
use spindle_kernel::External;

const LIBRARY: &str = "crates/spindle-host/tests/fixtures/refusal";
const CLOSE: &str = "library.lending.CloseLoan";
const THANK: &str = "library.notices.SendThanks";
const BINDING: &str = "thank-on-close";
const VIEWS: [&str; 2] = ["library.lending.Loans", "library.notices.AllThanks"];

/// The answers the notices give, one per attempt, before they send: each a scenario of its own.
const SCRIPTS: [&[&str]; 4] = [
    &["no-reader"],
    &["busy", "busy"],
    &["busy", "no-reader"],
    &[],
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn library() -> EssIr {
    spindle_spec::load(&root().join(LIBRARY))
        .expect("the refusal fixture loads")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// What one scenario observed: how many times the binding invoked `SendThanks`, and each view's
/// rows as JSON.
#[derive(Debug, PartialEq)]
struct Observed {
    attempts: usize,
    rows: BTreeMap<String, Vec<Value>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn interpreted(script: &[&str]) -> Observed {
    let target = Interpreted::for_model(library());
    let context = ScenarioContext::new(
        "library.notices/authored/refusal-policy"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("refusal-policy").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let thank = CommandRef::new(THANK.parse().expect("a command name"));
    target.script_binding_port(
        &thank,
        script.iter().map(|outcome| {
            PortAnswer::Outcome(OutcomeName::new(*outcome).expect("an outcome name"))
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

/// Answers each attempt of `SendThanks` with the next refusal of a script, and once it runs out
/// with none, as the interpreter's scripted port does. An attempt starts when an outcome is asked
/// about a second time.
#[derive(Default)]
struct Script {
    answers: VecDeque<String>,
    current: Option<Option<String>>,
    asked: BTreeSet<String>,
}

fn decider(script: &[&str]) -> impl FnMut(&External<'_>) -> Result<bool, String> + Send + 'static {
    let state = Arc::new(Mutex::new(Script {
        answers: script.iter().map(|outcome| (*outcome).to_owned()).collect(),
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
        Ok(script.current.as_ref().and_then(Option::as_deref) == Some(question.outcome))
    }
}

fn hosted(script: &[&str]) -> Observed {
    let mut host = Host::deciding(
        library(),
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

#[test]
fn refusal_policies_agree_with_the_interpreter() {
    let expected = [1, 3, 2, 1];
    for (script, attempts) in SCRIPTS.iter().zip(expected) {
        let ess = interpreted(script);
        assert_eq!(
            ess.attempts, attempts,
            "the interpreter, {script:?}: {ess:?}"
        );
        assert_eq!(hosted(script), ess, "{script:?}");
    }
}

/// Every scenario ESS synthesises for the fixture that the host executes answers as the
/// interpreter answers it.
#[test]
fn the_synthesised_refusal_scenarios_answer_as_the_interpreter() {
    let specification =
        spindle_spec::load(&root().join(LIBRARY)).expect("the refusal fixture loads");
    let synthesis = spindle_conformance::synthesize(&specification).expect("the suite synthesises");
    let ir = specification.ir;
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let statuses = |run: &spindle_conformance::Run| {
        run.report()
            .scenarios
            .iter()
            .map(|result| (result.scenario.to_string(), result.status))
            .collect::<BTreeMap<_, _>>()
    };
    let ess = statuses(&spindle_conformance::execute(
        &suite,
        &Interpreted::for_model(ir.clone()),
    ));
    let spindle = statuses(&spindle_conformance::execute(
        &suite,
        &HostTarget::new(ir, laptop()),
    ));
    let divergent = spindle
        .iter()
        .filter(|(_, status)| **status != Status::Unsupported)
        .filter(|(scenario, status)| ess.get(*scenario) != Some(status))
        .collect::<Vec<_>>();
    assert!(
        divergent.is_empty(),
        "ess: {ess:?}\nspindle: {spindle:?}\ndivergent: {divergent:?}"
    );
    assert!(
        spindle
            .values()
            .any(|status| *status != Status::Unsupported),
        "the host executes some scenario: {spindle:?}"
    );
}
