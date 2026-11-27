//! A binding whose failure policy escalates (story:binding-failure-escalates), run by the
//! in-process host and by ESS 0.99.1's interpreter on the same neutral fixture,
//! `crates/spindle-host/tests/fixtures/escalation`: placing an order ships it through the binding
//! `ship-on-order`, which drops `cancelled`, escalates `no-address` and `oversize` into
//! `depot.shipping.ShipmentStuck` and retries any other refusal; `flag-stuck` flags the order of a
//! stuck shipment. Variants of the fixture are written beside the test's build output.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef, EventRef, ViewRef};
use ess_conformance::interpret::{Interpreted, PortAnswer};
use ess_conformance::{
    ConformanceTarget, Deadline, EventObservationRequest, InvocationObservationRequest,
    ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Escalation, Host, Report, SystemClock};
use spindle_kernel::External;

const DEPOT: &str = "crates/spindle-host/tests/fixtures/escalation";
const PLACE: &str = "depot.orders.PlaceOrder";
const SHIP: &str = "depot.shipping.Ship";
const FLAG: &str = "depot.orders.FlagOrder";
const SHIPPING: &str = "ship-on-order";
const FLAGGING: &str = "flag-stuck";
const STUCK: &str = "depot.shipping.ShipmentStuck";
const VIEWS: [&str; 3] = [
    "depot.orders.Orders",
    "depot.orders.Flags",
    "depot.shipping.Shipments",
];

/// The policy of `ship-on-order` as the fixture writes it, which a variant replaces.
const PER_REFUSAL: &str = "      drop: [cancelled]
      escalate:
        emits: depot.shipping.ShipmentStuck
        outcomes: [no-address, oversize]
      retry: {except: [cancelled, no-address, oversize]}
";

/// The fields of `depot.shipping.ShipmentStuck` as the fixture declares them, which a variant
/// extends.
const STUCK_FIELDS: &str = "  - name: depot.shipping.ShipmentStuck
    fields:
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The fixture, or a copy of it under the test's build output named `name` with each `(from, to)`
/// of `system` replaced in `system.yaml` and each of `shipping` in `domains/shipping.yaml`.
fn depot(name: &str, system: &[(&str, &str)], shipping: &[(&str, &str)]) -> PathBuf {
    let fixture = root().join(DEPOT);
    if system.is_empty() && shipping.is_empty() {
        return fixture;
    }
    let copy = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("escalation-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(copy.join("domains")).expect("the copy's directory");
    for (file, replaced) in [
        ("system.yaml", system),
        ("domains/orders.yaml", &[][..]),
        ("domains/shipping.yaml", shipping),
    ] {
        let mut text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        for (from, to) in replaced {
            assert!(text.contains(from), "{file} holds {from:?}");
            text = text.replace(from, to);
        }
        std::fs::write(copy.join(file), text).expect("the copy is written");
    }
    copy
}

/// The fixture with `ship-on-order` escalating every failure: `on_failure: {escalate: ...}`.
fn universal() -> PathBuf {
    depot(
        "universal",
        &[(
            PER_REFUSAL,
            "      escalate:\n        emits: depot.shipping.ShipmentStuck\n",
        )],
        &[],
    )
}

/// The fixture with `depot.shipping.ShipmentStuck` carrying `carrier` of `kind`, a field
/// `depot.shipping.Ship`'s input does not hold.
fn carrying(kind: &str) -> PathBuf {
    let field = format!("{STUCK_FIELDS}      - name: carrier\n        type: {kind}\n");
    depot(
        &kind.replace(['<', '>'], "-"),
        &[],
        &[(STUCK_FIELDS, field.as_str())],
    )
}

fn load(spec: &Path) -> EssIr {
    spindle_spec::load(spec).expect("the fixture loads").ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// What one run observed after placing order `o-1`.
#[derive(Debug, PartialEq)]
struct Observed {
    /// How many times `ship-on-order` invoked `Ship`.
    shipped: usize,
    /// How many times `flag-stuck` invoked `FlagOrder`.
    flagged: usize,
    /// The payload of each `ShipmentStuck` published, in order.
    stuck: Vec<Value>,
    /// Each view's rows.
    rows: BTreeMap<String, Vec<Value>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn object_of(fields: &BTreeMap<String, Node>) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|(field, value)| (field.clone(), json_of(value)))
            .collect(),
    )
}

/// ESS's interpreter on `spec`, `Ship` answering each attempt with the next refusal of `script`
/// and then running as the model says.
fn interpreted(spec: &Path, script: &[&str]) -> Observed {
    let target = Interpreted::for_model(load(spec));
    let context = ScenarioContext::new(
        "depot.shipping/authored/escalation"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("escalation").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let ship = CommandRef::new(SHIP.parse().expect("a command name"));
    target.script_binding_port(
        &ship,
        script.iter().map(|outcome| {
            PortAnswer::Outcome(OutcomeName::new(*outcome).expect("an outcome name"))
        }),
    );
    let result = target
        .execute_command(SemanticCommandRequest {
            command: CommandRef::new(PLACE.parse().expect("a command name")),
            actor: None,
            caller: None,
            input: BTreeMap::from([("order_id".to_owned(), Node::Text("o-1".to_owned()))]),
            correlation: context.correlation.clone(),
        })
        .expect("the interpreter places the order");
    assert!(result.error.is_none(), "{result:?}");
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let invoked = |binding: &str, command: &str| {
        target
            .observe_invocations(InvocationObservationRequest {
                binding: binding.parse::<BindingRef>().expect("a binding"),
                command: CommandRef::new(command.parse().expect("a command name")),
                correlation: context.correlation.clone(),
                deadline,
            })
            .expect("the interpreter answers its invocations")
            .len()
    };
    let shipped = invoked(SHIPPING, SHIP);
    let flagged = invoked(FLAGGING, FLAG);
    let stuck = target
        .observe_events(EventObservationRequest {
            event: STUCK.parse::<EventRef>().expect("an event"),
            correlation: context.correlation.clone(),
            deadline,
        })
        .expect("the interpreter answers its events")
        .iter()
        .map(|event| object_of(&event.payload))
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
                .map(object_of)
                .collect();
            ((*view).to_owned(), rows)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    Observed {
        shipped,
        flagged,
        stuck,
        rows,
    }
}

/// Answers each attempt of `Ship` with the next refusal of a script, and once it runs out with
/// none, as the interpreter's scripted port does. An attempt starts when an outcome is asked about
/// a second time.
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
        assert_eq!(question.command, SHIP, "the only external causes");
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

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// The host on `spec`, `Ship` answered by `script`, after placing order `o-1`; its report, and
/// the report of one more [`Host::advance`].
fn hosted_report(spec: &Path, script: &[&str]) -> (Host, Report, Report) {
    let mut host = Host::deciding(
        load(spec),
        &laptop(),
        Arc::new(SystemClock),
        Path::new(""),
        Box::new(decider(script)),
    )
    .expect("the laptop admits the depot");
    let report = host
        .submit(PLACE, object(json!({"order_id": "o-1"})))
        .expect("the order is placed");
    let later = host.advance().expect("the host goes on");
    (host, report, later)
}

/// How many times `binding` invoked `command` in `report`, attempts counted.
fn attempts(report: &Report, binding: &str, command: &str) -> u32 {
    let by = |cause: &Cause, invoked: &str| {
        invoked == command
            && *cause
                == Cause::Delivered {
                    binding: binding.to_owned(),
                }
    };
    report
        .ran
        .iter()
        .filter(|ran| by(&ran.cause, &ran.command))
        .map(|ran| ran.attempts)
        .chain(
            report
                .refused
                .iter()
                .filter(|refused| by(&refused.cause, &refused.command))
                .map(|refused| refused.attempts),
        )
        .chain(
            report
                .failed
                .iter()
                .filter(|failed| by(&failed.cause, &failed.command))
                .map(|failed| failed.attempts),
        )
        .sum()
}

fn hosted(spec: &Path, script: &[&str]) -> Observed {
    let (mut host, report, later) = hosted_report(spec, script);
    let count = |binding: &str, command: &str| {
        usize::try_from(attempts(&report, binding, command) + attempts(&later, binding, command))
            .expect("a count")
    };
    let stuck = report
        .escalated
        .iter()
        .chain(&later.escalated)
        .filter_map(|escalated| match &escalated.escalation {
            Escalation::Published(payload) => Some(Value::Object(payload.clone())),
            Escalation::Unmet(_) => None,
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
    Observed {
        shipped: count(SHIPPING, SHIP),
        flagged: count(FLAGGING, FLAG),
        stuck,
        rows,
    }
}

#[test]
fn an_escalated_refusal_emits_the_named_event_and_invokes_nothing() {
    let spec = root().join(DEPOT);
    for script in [&["no-address"][..], &["oversize"], &["busy", "oversize"]] {
        let ess = interpreted(&spec, script);
        assert_eq!(
            ess.stuck,
            [json!({"order_id": "o-1"})],
            "the interpreter, {script:?}: {ess:?}"
        );
        assert_eq!(ess.shipped, script.len(), "the interpreter, {script:?}");
        let spindle = hosted(&spec, script);
        assert_eq!(spindle, ess, "{script:?}");
        let (_host, report, later) = hosted_report(&spec, script);
        assert_eq!(report.escalated.len(), 1, "{report:?}");
        let escalated = &report.escalated[0];
        assert_eq!(
            (
                escalated.binding.as_str(),
                escalated.component.as_str(),
                escalated.command.as_str(),
                escalated.event.as_str(),
                escalated.outcome.as_deref(),
            ),
            (SHIPPING, "shipping", SHIP, STUCK, script.last().copied()),
        );
        assert_eq!(
            attempts(&later, SHIPPING, SHIP),
            0,
            "no further invocation of the bound command follows: {later:?}"
        );
    }
}

#[test]
fn a_dropped_refusal_is_still_dropped() {
    let spec = root().join(DEPOT);
    for script in [&["cancelled"][..], &["busy", "cancelled"]] {
        let ess = interpreted(&spec, script);
        assert!(ess.stuck.is_empty(), "{script:?}: {ess:?}");
        let spindle = hosted(&spec, script);
        assert_eq!(spindle, ess, "{script:?}");
        let (_host, report, _later) = hosted_report(&spec, script);
        assert!(report.escalated.is_empty(), "{report:?}");
        assert_eq!(
            report.refused.len(),
            1,
            "reported once, as dropped: {report:?}"
        );
        assert_eq!(report.refused[0].outcome, "cancelled");
    }
}

#[test]
fn a_universal_escalation_runs() {
    let spec = universal();
    for script in [
        &["cancelled"][..],
        &["no-address"],
        &["oversize"],
        &["busy"],
    ] {
        let ess = interpreted(&spec, script);
        assert_eq!(
            ess.stuck,
            [json!({"order_id": "o-1"})],
            "the interpreter, {script:?}: {ess:?}"
        );
        assert_eq!(ess.shipped, 1, "the interpreter, {script:?}");
        assert_eq!(hosted(&spec, script), ess, "{script:?}");
    }
}

#[test]
fn an_escalation_event_field_the_input_lacks_is_reported_unmet() {
    let spec = carrying("String");
    let (mut host, report, later) = hosted_report(&spec, &["no-address"]);
    assert_eq!(report.escalated.len(), 1, "{report:?}");
    let escalated = &report.escalated[0];
    assert_eq!(escalated.event, STUCK);
    match &escalated.escalation {
        Escalation::Unmet(why) => assert!(why.contains("carrier"), "{why}"),
        Escalation::Published(payload) => panic!("published {payload:?}"),
    }
    assert_eq!(
        attempts(&report, FLAGGING, FLAG) + attempts(&later, FLAGGING, FLAG),
        0,
        "no event was published, so no binding invoked: {report:?}"
    );
    assert!(
        host.list("depot.orders.Flags")
            .expect("the flags read")
            .is_empty()
    );
    host.stop();
}

#[test]
fn an_optional_escalation_event_field_the_input_lacks_is_left_out() {
    let spec = carrying("Optional<String>");
    let (mut host, report, _later) = hosted_report(&spec, &["oversize"]);
    assert_eq!(report.escalated.len(), 1, "{report:?}");
    match &report.escalated[0].escalation {
        Escalation::Published(payload) => {
            assert_eq!(Value::Object(payload.clone()), json!({"order_id": "o-1"}));
        }
        Escalation::Unmet(why) => panic!("reported unmet: {why}"),
    }
    assert_eq!(attempts(&report, FLAGGING, FLAG), 1, "{report:?}");
    host.stop();
}

#[test]
fn an_escalation_event_reaches_a_binding_subscribed_to_it() {
    let spec = root().join(DEPOT);
    let ess = interpreted(&spec, &["no-address"]);
    assert_eq!(
        ess.flagged, 1,
        "the interpreter dispatches the escalation: {ess:?}"
    );
    assert_eq!(
        ess.rows["depot.orders.Flags"],
        [json!({"order_id": "o-1"})],
        "{ess:?}"
    );
    let spindle = hosted(&spec, &["no-address"]);
    assert_eq!(spindle.flagged, 1, "{spindle:?}");
    assert_eq!(spindle, ess);
}
