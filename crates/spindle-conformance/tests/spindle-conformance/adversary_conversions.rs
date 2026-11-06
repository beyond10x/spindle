//! Adversarial cases for a binding mapping across a declared conversion: the host against ESS
//! 0.87.0's interpreter when the converted value is refused and the event triggers a sibling
//! binding, and at the bound of the target newtype's invariant.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    ConformanceTarget, Deadline, InvocationObservationRequest, ScenarioContext,
    SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, SystemClock};

const FIXTURE: &str = "tests/fixtures/adversary-conversions";
const CLOSE: &str = "library.lending.CloseLoan";
const BINDINGS: [(&str, &str); 2] = [
    ("thank-on-close", "library.notices.SendThanks"),
    ("z-note-closure", "library.notices.NoteClosure"),
];
const VIEWS: [&str; 3] = [
    "library.lending.Loans",
    "library.notices.AllThanks",
    "library.notices.AllClosures",
];

fn ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// What one run of `CloseLoan` came to: whether the command itself was answered as taken, every
/// binding invocation in binding order, and each view's rows.
#[derive(Debug, PartialEq)]
struct Observed {
    answered: bool,
    invoked: Vec<(String, String)>,
    rows: BTreeMap<String, Vec<Value>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn interpreted(member: &str) -> (Observed, String) {
    let target = Interpreted::for_model(ir());
    let context = ScenarioContext::new(
        "library.lending/authored/adversary-conversion"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-conversion").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let result = target.execute_command(SemanticCommandRequest {
        command: CommandRef::new(CLOSE.parse().expect("a command name")),
        actor: None,
        caller: None,
        input: BTreeMap::from([
            ("loan_id".to_owned(), Node::Text("loan-1".to_owned())),
            ("member_id".to_owned(), Node::Text(member.to_owned())),
        ]),
        correlation: context.correlation.clone(),
    });
    let answer = format!("{result:?}");
    let answered = matches!(&result, Ok(result) if result.error.is_none());
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let mut invoked = Vec::new();
    for (binding, command) in BINDINGS {
        let invocations = target
            .observe_invocations(InvocationObservationRequest {
                binding: binding.parse::<BindingRef>().expect("a binding"),
                command: CommandRef::new(command.parse().expect("a command name")),
                correlation: context.correlation.clone(),
                deadline,
            })
            .expect("the interpreter answers its invocations");
        for invocation in invocations {
            invoked.push((
                invocation.binding.to_string(),
                invocation.command.to_string(),
            ));
        }
    }
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
    let _ = target.end_scenario(&context);
    (
        Observed {
            answered,
            invoked,
            rows,
        },
        answer,
    )
}

fn hosted(member: &str) -> (Observed, String) {
    let mut host = Host::new(ir(), &laptop(), Arc::new(SystemClock), Path::new(""))
        .expect("the laptop admits the fixture");
    let input = json!({ "loan_id": "loan-1", "member_id": member });
    let Value::Object(input) = input else {
        unreachable!("an object")
    };
    let submitted = host.submit(CLOSE, input);
    let answer = format!("{submitted:?}");
    let answered = submitted.is_ok();
    let mut invoked = submitted
        .as_ref()
        .map(|report| {
            report
                .ran
                .iter()
                .filter_map(|ran| match &ran.cause {
                    Cause::Delivered { binding } => Some((binding.clone(), ran.command.clone())),
                    Cause::Submitted => None,
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    invoked.sort();
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
    (
        Observed {
            answered,
            invoked,
            rows,
        },
        answer,
    )
}

/// The control: a value the target type admits runs both bindings, on both sides.
#[test]
fn an_admitted_converted_value_runs_both_bindings_on_both_sides() {
    let (ess, _) = interpreted("m-7");
    let (spindle, _) = hosted("m-7");
    assert_eq!(ess.invoked.len(), 2, "{ess:?}");
    assert_eq!(spindle, ess);
}

/// `member-0042` is no `ReaderRef`. ESS 0.87.0's interpreter answers the scenario unsupported
/// (`interpret/bindings.rs`: a binding without `on_refusal` whose mapped input does not satisfy its
/// type returns an error), so it defines no result here. Spindle has already committed `CloseLoan`
/// when the binding maps: it answers taken, runs the sibling `z-note-closure`, and reports
/// `SendThanks` as a command that did not execute (`docs/design/spindle.md` §11, unmet binding input).
#[test]
fn a_refused_converted_value_runs_its_siblings_where_the_interpreter_is_unsupported() {
    let (ess, ess_answer) = interpreted("member-0042");
    assert!(!ess.answered, "{ess_answer}");
    assert!(
        ess_answer.contains("Unsupported {")
            && ess_answer.contains("binding input does not satisfy its declared type"),
        "the interpreter answers unsupported: {ess_answer}"
    );

    let mut host = Host::new(ir(), &laptop(), Arc::new(SystemClock), Path::new(""))
        .expect("the laptop admits the fixture");
    let input = json!({ "loan_id": "loan-1", "member_id": "member-0042" });
    let Value::Object(input) = input else {
        unreachable!("an object")
    };
    let report = host.submit(CLOSE, input).expect("CloseLoan is taken");
    let delivered = report
        .ran
        .iter()
        .filter_map(|ran| match &ran.cause {
            Cause::Delivered { binding } => Some((binding.as_str(), ran.command.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        delivered,
        [("z-note-closure", "library.notices.NoteClosure")],
        "{report:?}"
    );
    assert_eq!(report.failed.len(), 1, "{report:?}");
    let failed = &report.failed[0];
    assert_eq!(failed.command, "library.notices.SendThanks");
    assert_eq!(
        failed.cause,
        Cause::Delivered {
            binding: "thank-on-close".to_owned()
        }
    );
    assert!(
        failed.error.contains("the input reader_id is unmet"),
        "{}",
        failed.error
    );
    assert!(
        host.list("library.notices.AllThanks")
            .expect("the host reads the view")
            .is_empty()
    );
    host.stop();
}

/// `value.count <= 8` at its bound, with a character that takes two bytes: eight characters is a
/// `ReaderRef` and nine is not, on both sides. Only whether `thank-on-close` was invoked is
/// compared, so the sibling divergence above does not count twice.
#[test]
fn the_invariant_bound_on_multibyte_text_agrees_with_the_interpreter() {
    for member in ["\u{fc}1234567", "\u{fc}12345678", "m-123456", "m-1234567"] {
        let thanked = |observed: &Observed| {
            observed
                .invoked
                .iter()
                .any(|(binding, _)| binding == "thank-on-close")
        };
        let (ess, ess_answer) = interpreted(member);
        let (spindle, spindle_answer) = hosted(member);
        assert_eq!(
            thanked(&spindle),
            thanked(&ess),
            "{member:?}\nspindle answered: {spindle_answer}\ninterpreter answered: {ess_answer}"
        );
        assert_eq!(
            spindle.rows["library.notices.AllThanks"], ess.rows["library.notices.AllThanks"],
            "{member:?}"
        );
    }
}
