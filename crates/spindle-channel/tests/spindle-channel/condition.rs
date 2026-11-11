//! A binding with a payload condition (`when: {event, where}`, ess/22) is routed, and the channel
//! decides it on each event's payload before it maps the input, as ESS 0.90.0's interpreter does
//! (`interpret/bindings.rs`, `holds`): True invokes, False skips the binding and reports nothing,
//! Unknown invokes nothing and is the binding's unmet obligation (story:binding-where-condition).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_channel::{Command, Event, Receiver, Router, Sender, Unmet, in_process};

fn parcels() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-cli/tests/fixtures/binding-where")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn event(name: &str, fields: Value) -> Event {
    Event {
        name: name.to_owned(),
        fields: object(fields),
    }
}

fn received(kind: &str) -> Event {
    event(
        "parcels.intake.ParcelReceived",
        json!({ "parcel_id": "p-1", "kind": kind }),
    )
}

fn parcels_router() -> Router {
    let loaded = spindle_spec::load(&parcels()).expect("the specification compiles");
    Router::from_ir(&loaded.ir).expect("a binding with a `where` condition is routable")
}

/// A sender for `from` connected to `to`, and the receiving end at `to`.
fn linked(router: Router, from: &str, to: &str) -> (Sender, Receiver) {
    let (near, far) = in_process::pair();
    let mut sender = Sender::new(from, router).expect("the component exists");
    sender.connect(to, near);
    (sender, Receiver::new(to, far))
}

/// Publishes `event` through `sender`, flushes it, and answers how many envelopes were staged,
/// the commands that arrived and the bindings held as unmet.
fn deliver(
    sender: &mut Sender,
    receiver: &mut Receiver,
    event: Event,
) -> (usize, Vec<Command>, Vec<Unmet>) {
    let staged = sender.publish(1, event).expect("the event is published");
    sender.mark_durable(1);
    sender.flush().expect("the flush succeeds");
    let mut arrived = Vec::new();
    while let Some(envelope) = receiver
        .receive(Duration::from_millis(20))
        .expect("the channel is open")
    {
        arrived.push(envelope.command);
    }
    (staged, arrived, sender.take_unmet())
}

#[test]
fn a_binding_with_a_where_condition_is_routed() {
    let router = parcels_router();
    let route = router.route("send-boxes").expect("send-boxes is routed");
    assert_eq!(route.event, "parcels.intake.ParcelReceived");
    assert_eq!(route.command, "parcels.dispatch.SendOn");
    assert_eq!(
        (route.from.as_str(), route.to.as_str()),
        ("intake", "dispatch")
    );
}

#[test]
fn an_event_whose_payload_matches_invokes_the_bound_command() {
    let (mut sender, mut receiver) = linked(parcels_router(), "intake", "dispatch");
    let (staged, arrived, unmet) = deliver(&mut sender, &mut receiver, received("box"));
    assert_eq!(staged, 1);
    assert_eq!(
        arrived,
        [Command {
            name: "parcels.dispatch.SendOn".to_owned(),
            fields: object(json!({ "parcel_id": "p-1" })),
        }]
    );
    assert!(unmet.is_empty(), "{unmet:?}");

    let mapped = parcels_router()
        .map(&received("box"))
        .expect("the event maps")
        .into_iter()
        .map(|(route, command)| (route.binding.clone(), command.name))
        .collect::<Vec<_>>();
    assert_eq!(
        mapped,
        [(
            "send-boxes".to_owned(),
            "parcels.dispatch.SendOn".to_owned()
        )]
    );
}

#[test]
fn an_event_whose_payload_does_not_match_invokes_nothing_and_reports_nothing() {
    let (mut sender, mut receiver) = linked(parcels_router(), "intake", "dispatch");
    let (staged, arrived, unmet) = deliver(&mut sender, &mut receiver, received("letter"));
    assert_eq!(staged, 0);
    assert!(arrived.is_empty(), "{arrived:?}");
    assert!(
        unmet.is_empty(),
        "a false condition is a skip, not a failure: {unmet:?}"
    );

    assert!(
        parcels_router()
            .map(&received("letter"))
            .expect("a false condition maps to nothing")
            .is_empty()
    );
}

/// One event, two bindings: `keep` reads an Optional label, which ESS's condition reads as
/// Unknown when it is absent, and `log` has no condition.
const UNKNOWN: &str = r"
format: ess/34
system: labels
version: v1
domain: labels.core
events:
  - name: labels.core.Arrived
    fields:
      - {name: item, type: String}
      - {name: label, type: Optional<String>}
  - name: labels.core.Kept
    fields:
      - {name: item, type: String}
commands:
  - name: labels.core.Arrive
    input:
      - {name: item, type: String}
      - {name: label, type: Optional<String>}
    outcomes:
      - name: arrived
        emits: [labels.core.Arrived]
        payload:
          labels.core.Arrived: {item: input.item, label: input.label}
  - name: labels.core.Keep
    input:
      - {name: item, type: String}
    outcomes:
      - name: kept
        emits: [labels.core.Kept]
        payload:
          labels.core.Kept: {item: input.item}
  - name: labels.core.Log
    input:
      - {name: item, type: String}
    outcomes:
      - name: logged
        emits: [labels.core.Kept]
        payload:
          labels.core.Kept: {item: input.item}
components:
  - component: labels-service
    owns:
      domains: [labels.core]
    accepts:
      commands: [labels.core.Arrive, labels.core.Keep, labels.core.Log]
    publishes:
      events: [labels.core.Arrived, labels.core.Kept]
bindings:
  - id: keep
    when:
      event: labels.core.Arrived
      where: event.label == fragile
    invoke: {command: labels.core.Keep}
    mapping:
      item: event.item
    delivery: at_most_once
    on_failure: drop
  - id: log
    when: {event: labels.core.Arrived}
    invoke: {command: labels.core.Log}
    mapping:
      item: event.item
    delivery: at_most_once
    on_failure: drop
";

fn labels_router() -> Router {
    let ir = spindle_spec::compile(&["labels.yaml".to_owned()], &[UNKNOWN.to_owned()])
        .expect("the inline specification compiles");
    Router::from_ir(&ir).expect("a binding with a `where` condition is routable")
}

fn arrived(label: Option<&str>) -> Event {
    match label {
        Some(label) => event(
            "labels.core.Arrived",
            json!({ "item": "i-1", "label": label }),
        ),
        None => event("labels.core.Arrived", json!({ "item": "i-1" })),
    }
}

fn names(commands: &[Command]) -> Vec<&str> {
    commands
        .iter()
        .map(|command| command.name.as_str())
        .collect()
}

#[test]
fn an_optional_field_decides_true_false_and_unknown_as_ess_does() {
    let (mut sender, mut receiver) = linked(labels_router(), "labels-service", "labels-service");
    let (staged, commands, unmet) = deliver(&mut sender, &mut receiver, arrived(Some("fragile")));
    assert_eq!(staged, 2);
    assert_eq!(names(&commands), ["labels.core.Keep", "labels.core.Log"]);
    assert!(unmet.is_empty(), "{unmet:?}");

    let (staged, commands, unmet) = deliver(&mut sender, &mut receiver, arrived(Some("sturdy")));
    assert_eq!(staged, 1, "a false condition skips only its own binding");
    assert_eq!(names(&commands), ["labels.core.Log"]);
    assert!(unmet.is_empty(), "{unmet:?}");

    // A comparison that reads an absent Optional field is Unknown: ESS invokes nothing and calls
    // it an unmet obligation, never a skip; the event's other bindings still run.
    let (staged, commands, unmet) = deliver(&mut sender, &mut receiver, arrived(None));
    assert_eq!(staged, 1);
    assert_eq!(names(&commands), ["labels.core.Log"]);
    assert_eq!(unmet.len(), 1, "{unmet:?}");
    assert_eq!(
        (
            unmet[0].binding.as_str(),
            unmet[0].to.as_str(),
            unmet[0].command.as_str()
        ),
        ("keep", "labels-service", "labels.core.Keep")
    );
    assert!(
        unmet[0].error.to_string().contains("condition"),
        "{}",
        unmet[0].error
    );
    assert!(
        labels_router().map(&arrived(None)).is_err(),
        "an unknown condition is no command"
    );

    // `null` reads as absent.
    let null = event(
        "labels.core.Arrived",
        json!({ "item": "i-1", "label": null }),
    );
    let (staged, _, unmet) = deliver(&mut sender, &mut receiver, null);
    assert_eq!((staged, unmet.len()), (1, 1), "{unmet:?}");
}
