//! Adversary pass 1 on `story:command-with-unpublished-event-served`.
//!
//! `Host::execute` now skips an emitted event its *own* component does not list under
//! `publishes:`. The story argues no delivery is dropped because "no binding can name such an
//! event (`Router::build` refuses one at start)". That holds for an event *no* component
//! publishes. ESS 0.99.1 does not tie `publishes:` to `owns:`, so another component may publish
//! an event the emitting component's domain declares; `Router::build` then routes a binding on it
//! (exactly one publisher), and ESS's interpreter, which dispatches bindings on every emitted
//! event whatever `publishes:` says, invokes that binding.
//!
//! The fixture `tests/fixtures/adversary-unpublished` is the story's help desk with two changes:
//! `log` publishes `helpdesk.tickets.Noted` (emitted by the desk's `TouchTicket`) and binds it to
//! `RecordMark` (`mark-on-note`); and the desk accepts `RemarkTicket`, which emits an unpublished
//! `Remarked`, the published `Touched`, then an unpublished `Opened`, in that order.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    ConformanceTarget, Deadline, InvocationObservationRequest, ScenarioContext, ScenarioId,
    SemanticCommandRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

const OPEN: &str = "helpdesk.tickets.OpenTicket";
const TOUCH: &str = "helpdesk.tickets.TouchTicket";
const REMARK: &str = "helpdesk.tickets.RemarkTicket";
const RECORD: &str = "helpdesk.log.RecordMark";
const MARKS: &str = "helpdesk.log.Marks";

fn ir() -> EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-unpublished"),
    )
    .unwrap_or_else(|error| panic!("ESS 0.99.1 compiles the adversary fixture: {error}"))
    .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-unpublished\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn ticket(id: &str) -> Map<String, Value> {
    let mut input = Map::new();
    input.insert("ticket_id".to_owned(), json!(id));
    input
}

fn host(ir: &EssIr, state: &Path) -> Host {
    Host::new(
        ir.clone(),
        &in_memory(),
        Arc::new(ManualClock::new()),
        state,
    )
    .unwrap_or_else(|error| panic!("the host admits the adversary help desk: {error}"))
}

/// The bindings ESS 0.99.1's interpreter invokes for `command` on `t1` after `OpenTicket` of `t1`.
fn interpreted_invocations(ir: EssIr, command: &str, bindings: &[&str]) -> Vec<String> {
    let interpreter = Interpreted::for_model(ir);
    let context = ScenarioContext::new(
        ScenarioId::parse("helpdesk.tickets.TouchTicket/authored/adversary").expect("a scenario"),
        CorrelationId::new("adversary").expect("a correlation id"),
    );
    interpreter
        .begin_scenario(&context)
        .expect("the scenario opens");
    for name in [OPEN, command] {
        interpreter
            .execute_command(SemanticCommandRequest {
                command: name.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: BTreeMap::from([("ticket_id".to_owned(), Node::Text("t1".to_owned()))]),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("ESS's interpreter executes {name}: {error}"));
    }
    let mut invoked = Vec::new();
    for binding in bindings {
        let seen = interpreter
            .observe_invocations(InvocationObservationRequest {
                binding: binding.parse::<BindingRef>().expect("a binding"),
                command: RECORD.parse::<CommandRef>().expect("a command"),
                correlation: context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the interpreter answers its invocations");
        for _ in seen {
            invoked.push((*binding).to_owned());
        }
    }
    let _ = interpreter.end_scenario(&context);
    invoked
}

/// `log` publishes `Noted` and binds it; the desk emits it beside `Touched`. ESS's interpreter
/// invokes both `mark-on-touch` and `mark-on-note`; the host must keep two marks, or refuse the
/// command, never answer it and drop the `Noted` delivery in silence.
#[test]
fn an_event_another_component_publishes_reaches_its_binding() {
    let ir = ir();
    let interpreted =
        interpreted_invocations(ir.clone(), TOUCH, &["mark-on-touch", "mark-on-note"]);
    assert_eq!(
        interpreted,
        ["mark-on-touch", "mark-on-note"],
        "ESS 0.99.1's interpreter invokes a binding on every emitted event"
    );

    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(&ir, state.path());
    host.submit(OPEN, ticket("t1")).expect("the ticket opens");
    let touched = host.submit(TOUCH, ticket("t1"));
    let marks = host.list(MARKS).expect("the marks are listed");
    assert!(
        touched.is_err() || marks.len() == 2,
        "the touch was served ({touched:?}) and the Noted that log publishes and mark-on-note \
         binds was dropped: {} mark(s), {marks:?}; ESS's interpreter invoked {interpreted:?}",
        marks.len()
    );
}

/// An unpublished event first, the published one between, another unpublished one last: one
/// envelope is sent, and `Host::published` lists the three in emission order after the `Opened`
/// of the open, then the `Marked` the delivered `Touched` led to.
#[test]
fn a_published_event_between_unpublished_ones_is_sent_in_order() {
    let ir = ir();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(&ir, state.path());
    host.submit(OPEN, ticket("t1")).expect("the ticket opens");
    host.submit(REMARK, ticket("t1"))
        .expect("the remark is served");
    host.submit(REMARK, ticket("t1"))
        .expect("a second remark is served");
    let marks = host.list(MARKS).expect("the marks are listed");
    assert_eq!(marks.len(), 2, "one mark per remark's Touched: {marks:?}");
    let names: Vec<String> = host
        .published()
        .expect("the published events read")
        .into_iter()
        .map(|published| published.event.name)
        .collect();
    assert_eq!(
        names,
        [
            "helpdesk.tickets.Opened",
            "helpdesk.tickets.Remarked",
            "helpdesk.tickets.Touched",
            "helpdesk.tickets.Opened",
            "helpdesk.log.Marked",
            "helpdesk.tickets.Remarked",
            "helpdesk.tickets.Touched",
            "helpdesk.tickets.Opened",
            "helpdesk.log.Marked",
        ],
    );
}
