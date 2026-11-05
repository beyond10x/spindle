//! A binding mapping across a declared conversion, run by the in-process host and by ESS 0.86.0's
//! interpreter on the same neutral fixture: closing a loan thanks its member as a reader.

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
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, SystemClock};

const LIBRARY: &str = "crates/spindle-channel/tests/fixtures/library";
const CLOSE: &str = "library.lending.CloseLoan";
const THANK: &str = "library.notices.SendThanks";
const BINDING: &str = "thank-on-close";
const VIEWS: [&str; 2] = ["library.lending.Loans", "library.notices.AllThanks"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn library() -> EssIr {
    spindle_spec::load(&root().join(LIBRARY))
        .expect("the library fixture loads")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// What one scenario observed: every command a binding invoked, and each view's rows as JSON. The
/// host reports no input beside an invocation; the rows are what the input wrote.
#[derive(Debug, PartialEq)]
struct Observed {
    invoked: Vec<(String, String)>,
    rows: BTreeMap<String, Vec<Value>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// The interpreter's observation, and the input it passed each invocation.
fn interpreted(member: &str) -> (Observed, Vec<Value>) {
    let target = Interpreted::for_model(library());
    let context = ScenarioContext::new(
        "library.lending/authored/converted-mapping"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("converted-mapping").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let result = target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(CLOSE.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: BTreeMap::from([
                ("loan_id".to_owned(), Node::Text("loan-1".to_owned())),
                ("member_id".to_owned(), Node::Text(member.to_owned())),
            ]),
            correlation: context.correlation.clone(),
        })
        .expect("the interpreter closes the loan");
    assert!(result.error.is_none(), "{result:?}");
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let invocations = target
        .observe_invocations(InvocationObservationRequest {
            binding: BINDING.parse::<BindingRef>().expect("a binding"),
            command: CommandRef::new(THANK.parse().expect("a command name")),
            correlation: context.correlation.clone(),
            deadline,
        })
        .expect("the interpreter answers its invocations");
    let invoked = invocations
        .iter()
        .map(|invocation| {
            (
                invocation.binding.to_string(),
                invocation.command.to_string(),
            )
        })
        .collect();
    let inputs = invocations
        .iter()
        .map(|invocation| {
            Value::Object(
                invocation
                    .input
                    .iter()
                    .map(|(field, value)| (field.clone(), json_of(value)))
                    .collect::<Map<_, _>>(),
            )
        })
        .collect();
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
    (Observed { invoked, rows }, inputs)
}

fn host() -> Host {
    Host::new(library(), &laptop(), Arc::new(SystemClock), Path::new(""))
        .expect("the laptop admits the library")
}

fn close(host: &mut Host, member: &str) -> spindle_host::Report {
    let input = json!({ "loan_id": "loan-1", "member_id": member });
    let Value::Object(input) = input else {
        unreachable!("an object")
    };
    host.submit(CLOSE, input).expect("the loan closes")
}

fn hosted(member: &str) -> Observed {
    let mut host = host();
    let report = close(&mut host, member);
    let invoked = report
        .ran
        .iter()
        .filter_map(|ran| match &ran.cause {
            Cause::Delivered { binding } => Some((binding.clone(), ran.command.clone())),
            _ => None,
        })
        .collect();
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
    Observed { invoked, rows }
}

#[test]
fn converted_mappings_agree_with_the_interpreter() {
    let (ess, inputs) = interpreted("m-7");
    assert_eq!(
        ess.invoked,
        [(BINDING.to_owned(), THANK.to_owned())],
        "the interpreter thanks the member"
    );
    assert_eq!(
        inputs,
        [json!({ "reader_id": "m-7" })],
        "as a reader, the member's identity carried unchanged"
    );
    assert_eq!(hosted("m-7"), ess);
}

/// The host reports a binding whose converted value its target type refuses as a command that did
/// not execute, naming the binding; nothing is written for it, and the command that published the
/// event still took effect.
#[test]
fn a_value_the_target_type_refuses_is_reported_by_the_host() {
    let mut host = host();
    let report = close(&mut host, "member-0042");
    assert_eq!(report.ran.len(), 1, "only the close ran: {report:?}");
    assert_eq!(report.ran[0].command, CLOSE);
    assert_eq!(report.failed.len(), 1, "{report:?}");
    let failed = &report.failed[0];
    assert_eq!(failed.component, "notices");
    assert_eq!(failed.command, THANK);
    assert_eq!(
        failed.cause,
        Cause::Delivered {
            binding: BINDING.to_owned()
        }
    );
    assert!(failed.error.contains("reader_id"), "{}", failed.error);
    assert!(
        host.list("library.notices.AllThanks")
            .expect("the host reads the view")
            .is_empty()
    );
    assert_eq!(
        host.list("library.lending.Loans")
            .expect("the host reads the view")
            .len(),
        1
    );
    host.stop();
}
