//! Adversary pass 1 on story:binding-where-condition: the in-process host against ESS 0.90.0's
//! interpreter on one event that triggers eight conditioned bindings at once — enum `!=`, an
//! Optional String, a Boolean, an Optional Boolean, a nested path through an Optional struct,
//! `any` and `not` over Unknown operands, and `defined` admitting an Optional source into a
//! required input — for payloads that make each condition true, false and Unknown.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_compiler::refs::{BindingRef, CommandRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    ConformanceTarget, Deadline, InvocationObservationRequest, ScenarioContext,
    SemanticCommandRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, SystemClock};

const LAPTOP: &str = "examples/environments/laptop.toml";
const PLACE: &str = "probe.core.Place";

const PROBE: &str = r"
format: ess/34
system: probe
version: v1
summary: Probe of binding conditions.
domain: probe.core
types:
  - name: probe.core.Kind
    kind: enum
    variants: [a, b]
  - name: probe.core.Address
    kind: struct
    fields:
      - {name: city, type: String}
      - {name: zone, type: Optional<String>}
events:
  - name: probe.core.Placed
    fields:
      - {name: id, type: String}
      - {name: kind, type: probe.core.Kind}
      - {name: note, type: Optional<String>}
      - {name: rush, type: Boolean}
      - {name: gift, type: Optional<Boolean>}
      - {name: addr, type: Optional<probe.core.Address>}
      - {name: amount, type: Optional<Integer>}
  - name: probe.core.Done
    fields:
      - {name: id, type: String}
commands:
  - name: probe.core.Place
    input:
      - {name: id, type: String}
      - {name: kind, type: probe.core.Kind}
      - {name: note, type: Optional<String>}
      - {name: rush, type: Boolean}
      - {name: gift, type: Optional<Boolean>}
      - {name: addr, type: Optional<probe.core.Address>}
      - {name: amount, type: Optional<Integer>}
    outcomes:
      - name: placed
        emits: [probe.core.Placed]
        payload:
          probe.core.Placed: {id: input.id, kind: input.kind, note: input.note, rush: input.rush, gift: input.gift, addr: input.addr, amount: input.amount}
  - name: probe.core.OnEnum
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnNote
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnRush
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnGift
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnZone
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnAny
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnNot
    input: [{name: id, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
  - name: probe.core.OnDefined
    input: [{name: id, type: String}, {name: note, type: String}]
    outcomes: [{name: done, emits: [probe.core.Done], payload: {probe.core.Done: {id: input.id}}}]
components:
  - component: probe-service
    owns:
      domains: [probe.core]
    accepts:
      commands: [probe.core.Place, probe.core.OnEnum, probe.core.OnNote, probe.core.OnRush, probe.core.OnGift, probe.core.OnZone, probe.core.OnAny, probe.core.OnNot, probe.core.OnDefined]
    publishes:
      events: [probe.core.Placed, probe.core.Done]
bindings:
  - id: on-enum
    when: {event: probe.core.Placed, where: event.kind != a}
    invoke: {command: probe.core.OnEnum}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-note
    when: {event: probe.core.Placed, where: event.note == x}
    invoke: {command: probe.core.OnNote}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-rush
    when: {event: probe.core.Placed, where: event.rush == true}
    invoke: {command: probe.core.OnRush}
    mapping: {id: event.id}
    delivery: at_least_once
    on_failure: retry
  - id: on-gift
    when: {event: probe.core.Placed, where: event.gift == false}
    invoke: {command: probe.core.OnGift}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-zone
    when: {event: probe.core.Placed, where: event.addr.zone == north}
    invoke: {command: probe.core.OnZone}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-any
    when: {event: probe.core.Placed, where: {any: [event.note == x, event.kind == a]}}
    invoke: {command: probe.core.OnAny}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-not
    when: {event: probe.core.Placed, where: {not: [defined(event.addr), event.addr.city == berlin, defined(event.amount)]}}
    invoke: {command: probe.core.OnNot}
    mapping: {id: event.id}
    delivery: at_most_once
    on_failure: drop
  - id: on-defined
    when: {event: probe.core.Placed, where: defined(event.note)}
    invoke: {command: probe.core.OnDefined}
    mapping: {id: event.id, note: event.note}
    delivery: at_most_once
    on_failure: drop
";

const BINDINGS: [(&str, &str); 8] = [
    ("on-enum", "probe.core.OnEnum"),
    ("on-note", "probe.core.OnNote"),
    ("on-rush", "probe.core.OnRush"),
    ("on-gift", "probe.core.OnGift"),
    ("on-zone", "probe.core.OnZone"),
    ("on-any", "probe.core.OnAny"),
    ("on-not", "probe.core.OnNot"),
    ("on-defined", "probe.core.OnDefined"),
];

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn probe() -> EssIr {
    spindle_spec::compile(&["probe.yaml".to_owned()], &[PROBE.to_owned()])
        .expect("the probe specification compiles")
}

fn node(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a node")
}

/// Which bindings invoked their command, and whether any condition was left unmet.
type Seen = (BTreeSet<String>, bool);

fn interpreted(input: &Map<String, Value>, case: usize) -> Seen {
    let target = Interpreted::for_model(probe());
    let context = ScenarioContext::new(
        "probe.core/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new(format!("case-{case}")).expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let result = target.execute_command(SemanticCommandRequest {
        command: CommandRef::new(PLACE.parse().expect("a command name")),
        actor: None,
        caller: None,
        input: input
            .iter()
            .map(|(name, value)| (name.clone(), node(value)))
            .collect::<BTreeMap<_, _>>(),
        correlation: context.correlation.clone(),
    });
    let unmet = match &result {
        Ok(_) => false,
        Err(error) => {
            assert!(
                error.to_string().contains("condition is unknown"),
                "case {case}: the interpreter failed otherwise: {error}"
            );
            true
        }
    };
    let mut invoked = BTreeSet::new();
    for (binding, command) in BINDINGS {
        let seen = target
            .observe_invocations(InvocationObservationRequest {
                binding: binding.parse::<BindingRef>().expect("a binding"),
                command: CommandRef::new(command.parse().expect("a command name")),
                correlation: context.correlation.clone(),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .expect("the interpreter answers its invocations");
        if !seen.is_empty() {
            invoked.insert(binding.to_owned());
        }
    }
    target.end_scenario(&context).expect("the scenario closes");
    (invoked, unmet)
}

fn hosted(host: &mut Host, input: &Map<String, Value>) -> (Seen, BTreeSet<String>) {
    let report = host
        .submit(PLACE, input.clone())
        .expect("the parcel is placed");
    let invoked = report
        .ran
        .iter()
        .filter_map(|ran| match &ran.cause {
            Cause::Delivered { binding } => Some(binding.clone()),
            _ => None,
        })
        .collect();
    let unmet = report
        .failed
        .iter()
        .filter_map(|failed| match &failed.cause {
            Cause::Delivered { binding } if failed.attempts == 0 => Some(binding.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    ((invoked, !unmet.is_empty()), unmet)
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn every_condition_shape_decides_as_ess_interprets_it() {
    let text = std::fs::read_to_string(root().join(LAPTOP)).expect("the laptop environment");
    let laptop = spindle_conformance::in_memory(
        &Environment::parse(&text).expect("the laptop environment parses"),
    );
    let mut host = Host::new(probe(), &laptop, Arc::new(SystemClock), Path::new(""))
        .expect("the probe starts");
    let cases = [
        json!({"id": "c0", "kind": "a", "rush": true}),
        json!({"id": "c1", "kind": "b", "note": "x", "rush": false, "gift": false,
               "addr": {"city": "berlin", "zone": "north"}, "amount": 3}),
        json!({"id": "c2", "kind": "b", "note": "y", "rush": false, "gift": true,
               "addr": {"city": "paris"}}),
        json!({"id": "c3", "kind": "a", "note": "x", "rush": true, "gift": false,
               "addr": {"city": "berlin", "zone": "south"}}),
        json!({"id": "c4", "kind": "a", "note": "x", "rush": true, "gift": true,
               "addr": {"city": "berlin", "zone": "north"}, "amount": 0}),
        json!({"id": "c5", "kind": "b", "rush": false}),
    ];
    // Worked by hand from ESS's Kleene rules (`ess-domain` `binding/condition.rs`): which
    // bindings invoke, and whether some condition is Unknown.
    let expected: [(&[&str], bool); 6] = [
        (&["on-any", "on-not", "on-rush"], true),
        (
            &[
                "on-any",
                "on-defined",
                "on-enum",
                "on-gift",
                "on-note",
                "on-zone",
            ],
            false,
        ),
        (&["on-defined", "on-enum", "on-not"], true),
        (
            &[
                "on-any",
                "on-defined",
                "on-gift",
                "on-not",
                "on-note",
                "on-rush",
            ],
            false,
        ),
        (
            &["on-any", "on-defined", "on-note", "on-rush", "on-zone"],
            false,
        ),
        (&["on-enum", "on-not"], true),
    ];
    let mut differences = Vec::new();
    for (case, input) in cases.into_iter().enumerate() {
        let input = object(input);
        let ess = interpreted(&input, case);
        let (here, unmet) = hosted(&mut host, &input);
        let (invoked, undecided) = expected[case];
        let literal = (
            invoked
                .iter()
                .map(|b| (*b).to_owned())
                .collect::<BTreeSet<_>>(),
            undecided,
        );
        if here != ess || here != literal {
            differences.push(format!(
                "case {case}: host invoked {:?} unmet {unmet:?}; ESS invoked {:?} unmet {}; \
                 by hand {literal:?}",
                here.0, ess.0, ess.1
            ));
        }
    }
    host.stop();
    assert!(differences.is_empty(), "{differences:#?}");
}

/// `null` for an Optional input, at the top level and inside a struct: the host's event carries
/// it, and ESS's condition reads it as absent.
#[test]
fn a_null_optional_decides_as_ess_interprets_it() {
    let text = std::fs::read_to_string(root().join(LAPTOP)).expect("the laptop environment");
    let laptop = spindle_conformance::in_memory(
        &Environment::parse(&text).expect("the laptop environment parses"),
    );
    let mut host = Host::new(probe(), &laptop, Arc::new(SystemClock), Path::new(""))
        .expect("the probe starts");
    let input = object(json!({
        "id": "n0", "kind": "b", "note": null, "rush": false, "gift": null,
        "addr": {"city": "berlin", "zone": null}, "amount": null
    }));
    let ess = interpreted(&input, 100);
    let (here, unmet) = hosted(&mut host, &input);
    host.stop();
    assert_eq!(here, ess, "host unmet {unmet:?}");
}
