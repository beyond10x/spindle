//! Adversary pass on `story:binding-failure-escalates` (wave 29 U1): the in-process host against
//! ESS 0.99.1's interpreter on escalation shapes the unit's own cases leave out.
//!
//! Attacked here: an attempt that fails with no declared outcome (ESS's `PortAnswer::Untyped`,
//! a decider that answers nothing here) under the universal escalation and under the fixture's
//! per-refusal table, whose fallback retries; a table that escalates one outcome and drops the rest;
//! a table whose fallback escalates; and `--drop-retries` on the host (the unit tests it only on
//! stdin). Variants of `crates/spindle-host/tests/fixtures/escalation` are written beside the
//! test's build output.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef, EventRef};
use ess_conformance::interpret::{Interpreted, PortAnswer};
use ess_conformance::{
    ConformanceTarget, Deadline, EventObservationRequest, InvocationObservationRequest,
    ScenarioContext, SemanticCommandRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Escalation, Host, Report, Retries, SystemClock};
use spindle_kernel::External;

const DEPOT: &str = "crates/spindle-host/tests/fixtures/escalation";
const PLACE: &str = "depot.orders.PlaceOrder";
const SHIP: &str = "depot.shipping.Ship";
const FLAG: &str = "depot.orders.FlagOrder";
const SHIPPING: &str = "ship-on-order";
const FLAGGING: &str = "flag-stuck";
const STUCK: &str = "depot.shipping.ShipmentStuck";
/// A script entry that fails the attempt with no declared outcome.
const UNTYPED: &str = "!untyped";

/// The policy of `ship-on-order` as the fixture writes it.
const PER_REFUSAL: &str = "      drop: [cancelled]
      escalate:
        emits: depot.shipping.ShipmentStuck
        outcomes: [no-address, oversize]
      retry: {except: [cancelled, no-address, oversize]}
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The fixture, or a copy named `name` with [`PER_REFUSAL`] replaced by `policy`.
fn depot(name: &str, policy: Option<&str>) -> PathBuf {
    let fixture = root().join(DEPOT);
    let Some(policy) = policy else {
        return fixture;
    };
    let copy = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("adversary-w29-u1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(copy.join("domains")).expect("the copy's directory");
    for file in [
        "system.yaml",
        "domains/orders.yaml",
        "domains/shipping.yaml",
    ] {
        let mut text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        if file == "system.yaml" {
            assert!(text.contains(PER_REFUSAL), "the fixture holds its policy");
            text = text.replace(PER_REFUSAL, policy);
        }
        std::fs::write(copy.join(file), text).expect("the copy is written");
    }
    copy
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
    shipped: usize,
    flagged: usize,
    stuck: Vec<Value>,
}

fn object_of(fields: &BTreeMap<String, Node>) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|(field, value)| {
                (
                    field.clone(),
                    serde_json::to_value(value).expect("a node is JSON"),
                )
            })
            .collect(),
    )
}

fn port(answer: &str) -> PortAnswer {
    if answer == UNTYPED {
        PortAnswer::Untyped
    } else {
        PortAnswer::Outcome(OutcomeName::new(answer).expect("an outcome name"))
    }
}

/// ESS's interpreter on `spec`, `Ship` answering each attempt with the next entry of `script`.
fn interpreted(spec: &Path, script: &[&str]) -> Observed {
    let target = Interpreted::for_model(load(spec));
    let context = ScenarioContext::new(
        "depot.shipping/authored/adversary-w29-u1"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w29-u1").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    target.script_binding_port(
        &CommandRef::new(SHIP.parse().expect("a command name")),
        script.iter().map(|answer| port(answer)),
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
    target.end_scenario(&context).expect("the scenario closes");
    Observed {
        shipped,
        flagged,
        stuck,
    }
}

#[derive(Default)]
struct Script {
    answers: VecDeque<String>,
    current: Option<Option<String>>,
    asked: BTreeSet<String>,
}

/// Answers each attempt of `Ship` with the next entry of `script`: an outcome taken, or
/// [`UNTYPED`], which answers nothing so the attempt fails with no declared outcome; once the
/// script runs out, no outcome is taken. An attempt starts when an outcome is asked about a
/// second time.
fn decider(script: &[&str]) -> impl FnMut(&External<'_>) -> Result<bool, String> + Send + 'static {
    let state = Arc::new(Mutex::new(Script {
        answers: script.iter().map(|answer| (*answer).to_owned()).collect(),
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
        let current = script.current.as_ref().and_then(Option::as_deref);
        if current == Some(UNTYPED) {
            return Err("the port fails the attempt".to_owned());
        }
        Ok(current == Some(question.outcome))
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn hosted_report(spec: &Path, script: &[&str], retries: Retries) -> (Host, Report, Report) {
    let mut host = Host::deciding_with_retries(
        load(spec),
        &laptop(),
        Arc::new(SystemClock),
        Path::new(""),
        Box::new(decider(script)),
        retries,
    )
    .expect("the laptop admits the depot");
    let report = host
        .submit(PLACE, object(json!({"order_id": "o-1"})))
        .expect("the order is placed");
    let later = host.advance().expect("the host goes on");
    (host, report, later)
}

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

fn hosted(spec: &Path, script: &[&str], retries: Retries) -> Observed {
    let (mut host, report, later) = hosted_report(spec, script, retries);
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
    host.stop();
    Observed {
        shipped: count(SHIPPING, SHIP),
        flagged: count(FLAGGING, FLAG),
        stuck,
    }
}

/// Both sides on `spec` for each script; the host's observation equals the interpreter's.
fn agree(spec: &Path, scripts: &[&[&str]]) -> Vec<Observed> {
    scripts
        .iter()
        .map(|script| {
            let ess = interpreted(spec, script);
            let spindle = hosted(spec, script, Retries::Retried);
            assert_eq!(spindle, ess, "host against the interpreter, {script:?}");
            ess
        })
        .collect()
}

#[test]
fn a_universal_escalation_escalates_an_attempt_with_no_declared_outcome() {
    let spec = depot(
        "universal-untyped",
        Some("      escalate:\n        emits: depot.shipping.ShipmentStuck\n"),
    );
    let observed = agree(&spec, &[&[UNTYPED]]);
    assert_eq!(
        observed[0],
        Observed {
            shipped: 1,
            flagged: 1,
            stuck: vec![json!({"order_id": "o-1"})],
        }
    );
}

#[test]
fn the_tables_retrying_fallback_answers_an_attempt_with_no_declared_outcome() {
    let spec = depot("per-refusal-untyped", None);
    let observed = agree(&spec, &[&[UNTYPED, "no-address"], &[UNTYPED, UNTYPED]]);
    assert_eq!(observed[0].shipped, 2, "{:?}", observed[0]);
    assert_eq!(observed[0].stuck, [json!({"order_id": "o-1"})]);
    assert_eq!(observed[1].shipped, 3, "{:?}", observed[1]);
    assert!(observed[1].stuck.is_empty(), "{:?}", observed[1]);
}

#[test]
fn a_table_that_escalates_one_outcome_and_drops_the_rest_agrees_with_the_interpreter() {
    let spec = depot(
        "escalate-named",
        Some(
            "      escalate:\n        emits: depot.shipping.ShipmentStuck\n        outcomes: [no-address]\n      drop: {except: [no-address]}\n",
        ),
    );
    let observed = agree(
        &spec,
        &[&["no-address"], &["busy"], &["cancelled"], &[UNTYPED]],
    );
    assert_eq!(observed[0].stuck, [json!({"order_id": "o-1"})]);
}

#[test]
fn a_table_whose_fallback_escalates_agrees_with_the_interpreter() {
    let spec = depot(
        "escalate-fallback",
        Some(
            "      drop: [cancelled]\n      escalate:\n        emits: depot.shipping.ShipmentStuck\n        except: [cancelled]\n",
        ),
    );
    let observed = agree(&spec, &[&["cancelled"], &["busy"], &[UNTYPED]]);
    assert!(observed[0].stuck.is_empty(), "{:?}", observed[0]);
    assert_eq!(observed[1].stuck, [json!({"order_id": "o-1"})]);
    assert_eq!(observed[2].stuck, [json!({"order_id": "o-1"})]);
}

#[test]
fn a_host_dropping_retries_still_escalates() {
    let spec = depot("dropping", None);
    let escalated = hosted(&spec, &["busy", "oversize"], Retries::Dropped);
    assert_eq!(
        escalated,
        Observed {
            shipped: 1,
            flagged: 0,
            stuck: Vec::new(),
        },
        "busy is retried in the table, so dropping retries drops it after one attempt"
    );
    let escalated = hosted(&spec, &["oversize"], Retries::Dropped);
    assert_eq!(
        escalated,
        Observed {
            shipped: 1,
            flagged: 1,
            stuck: vec![json!({"order_id": "o-1"})],
        }
    );
}
