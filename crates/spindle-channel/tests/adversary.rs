use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ess_compiler::ir::ResolvedMappingValue;
use serde_json::{Map, Value, json};
use spindle_channel::{
    Channel, ChannelError, Command, DeliveryId, Envelope, Event, Frame, Receiver, Router, Sender,
    in_process, tcp::MAX_FRAME, tcp::TcpChannel,
};

const PATIENCE: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(50);

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

const FAN_SYSTEM: &str = r"
format: ess/34
system: fan
version: v1
domains:
  - fan.source
  - fan.alpha
  - fan.beta
";

const FAN_COMPONENTS: &str = r"
components:
  - component: fan-source
    owns:
      domains: [fan.source]
    accepts:
      commands: [fan.source.Shout]
    publishes:
      events: [fan.source.Shouted]
  - component: a-sink
    owns:
      domains: [fan.alpha]
    accepts:
      commands: [fan.alpha.Hear]
    publishes:
      events: [fan.alpha.Heard]
  - component: b-sink
    owns:
      domains: [fan.beta]
    accepts:
      commands: [fan.beta.Hear]
    publishes:
      events: [fan.beta.Heard]
bindings:
  - id: alpha-hears
    when:
      event: fan.source.Shouted
    invoke:
      command: fan.alpha.Hear
    mapping:
      word: event.word
    delivery: at_least_once
    on_failure: drop
  - id: beta-hears
    when:
      event: fan.source.Shouted
    invoke:
      command: fan.beta.Hear
    mapping:
      word: event.word
    delivery: at_least_once
    on_failure: drop
";

const FAN_SOURCE: &str = r"
domain: fan.source
events:
  - name: fan.source.Shouted
    fields:
      - name: word
        type: String
commands:
  - name: fan.source.Shout
    input:
      - name: word
        type: String
    outcomes:
      - name: shouted
        emits: [fan.source.Shouted]
        payload:
          fan.source.Shouted:
            word: input.word
";

fn fan_sink(domain: &str) -> String {
    format!(
        r"
domain: fan.{domain}
events:
  - name: fan.{domain}.Heard
    fields:
      - name: word
        type: String
commands:
  - name: fan.{domain}.Hear
    input:
      - name: word
        type: String
    outcomes:
      - name: heard
        emits: [fan.{domain}.Heard]
        payload:
          fan.{domain}.Heard:
            word: input.word
"
    )
}

fn fan_router() -> Router {
    let identities = [
        "system.yaml",
        "components.yaml",
        "source.yaml",
        "alpha.yaml",
        "beta.yaml",
    ]
    .map(str::to_owned);
    let texts = [
        FAN_SYSTEM.to_owned(),
        FAN_COMPONENTS.to_owned(),
        FAN_SOURCE.to_owned(),
        fan_sink("alpha"),
        fan_sink("beta"),
    ];
    let ir = spindle_spec::compile(&identities, &texts).expect("the fan specification compiles");
    Router::from_ir(&ir).expect("every event binding is routable")
}

fn shouted(word: &str) -> Event {
    event("fan.source.Shouted", json!({ "word": word }))
}

#[test]
fn an_acknowledgement_is_collected_while_another_destination_is_closed() {
    let (to_alpha, alpha_end) = in_process::pair();
    let (to_beta, beta_end) = in_process::pair();
    let mut source = Sender::new("fan-source", fan_router())
        .expect("fan-source exists")
        .with_acknowledgement_timeout(Duration::from_secs(2));
    source.connect("a-sink", to_alpha);
    source.connect("b-sink", to_beta);
    let mut alpha = Receiver::new("a-sink", alpha_end);
    let mut beta = Receiver::new("b-sink", beta_end);

    assert_eq!(source.publish(1, shouted("hi")).expect("staged"), 2);
    source.mark_durable(1);
    assert_eq!(source.flush().expect("sent"), 2);

    let to_a = alpha
        .receive(PATIENCE)
        .expect("open")
        .expect("alpha receives");
    let to_b = beta
        .receive(PATIENCE)
        .expect("open")
        .expect("beta receives");
    assert_ne!(to_a.id, to_b.id, "each binding has its own envelope");
    assert_eq!(source.unacknowledged(), 2);

    drop(alpha);
    beta.acknowledge(&to_b).expect("beta acknowledges");

    let deadline = Instant::now() + PATIENCE;
    while source.unacknowledged() > 1 && Instant::now() < deadline {
        let _ = source.collect_acknowledgements(QUIET);
    }
    assert_eq!(
        source.unacknowledged(),
        1,
        "beta's acknowledgement is settled although a-sink is gone; only a-sink's envelope stays in flight"
    );
}

#[test]
fn a_tcp_receive_with_an_unbounded_wait_returns_the_waiting_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let mut near = TcpChannel::connect(listener.local_addr().expect("bound")).expect("connects");
    let (stream, _) = listener.accept().expect("accepts");
    let mut far = TcpChannel::new(stream).expect("a channel");
    near.send(Frame::Acknowledge(DeliveryId(3))).expect("sent");

    let (in_near, mut in_far) = in_process::pair();
    let mut in_near = in_near;
    in_near
        .send(Frame::Acknowledge(DeliveryId(3)))
        .expect("sent");
    assert_eq!(
        in_far.receive(Duration::MAX).expect("open"),
        Some(Frame::Acknowledge(DeliveryId(3))),
        "the in-process channel accepts an unbounded wait"
    );

    assert_eq!(
        far.receive(Duration::MAX).expect("open"),
        Some(Frame::Acknowledge(DeliveryId(3))),
        "the TCP channel behind the same port accepts it too"
    );
}

#[test]
fn redelivery_under_an_unbounded_acknowledgement_timeout_waits_instead_of_panicking() {
    let (to_alpha, alpha_end) = in_process::pair();
    let (to_beta, _beta_end) = in_process::pair();
    let mut source = Sender::new("fan-source", fan_router())
        .expect("fan-source exists")
        .with_acknowledgement_timeout(Duration::MAX);
    source.connect("a-sink", to_alpha);
    source.connect("b-sink", to_beta);
    let _alpha = Receiver::new("a-sink", alpha_end);

    source.publish(1, shouted("hi")).expect("staged");
    source.mark_durable(1);
    source.flush().expect("sent");
    assert_eq!(
        source.redeliver(Instant::now()).expect("open"),
        0,
        "an acknowledgement that may take forever is never overdue"
    );
}

const RETRYING: &str = r"
format: ess/34
system: retrying
version: v1
domain: retrying.core
events:
  - name: retrying.core.Arrived
    fields:
      - {name: note, type: String}
  - name: retrying.core.Done
    fields:
      - {name: note, type: String}
commands:
  - name: retrying.core.Publish
    input:
      - {name: note, type: String}
    outcomes:
      - name: accepted
        emits: [retrying.core.Arrived]
        payload:
          retrying.core.Arrived: {note: input.note}
  - name: retrying.core.Receive
    input:
      - {name: note, type: String}
      - {name: price, type: Decimal}
    outcomes:
      - name: accepted
        emits: [retrying.core.Done]
        payload:
          retrying.core.Done: {note: input.note}
components:
  - component: retrying-service
    owns:
      domains: [retrying.core]
    accepts:
      commands: [retrying.core.Publish, retrying.core.Receive]
    publishes:
      events: [retrying.core.Arrived, retrying.core.Done]
bindings:
  - id: receive
    when: {event: retrying.core.Arrived}
    invoke: {command: retrying.core.Receive}
    mapping:
      note: event.note
      price: 1.10
    delivery: at_most_once
    on_failure: retry
";

fn retrying_ir() -> ess_compiler::EssIr {
    spindle_spec::compile(&["retrying.yaml".to_owned()], &[RETRYING.to_owned()])
        .expect("the inline specification compiles")
}

#[test]
fn a_decimal_literal_is_carried_as_the_decimal_string_its_schema_types() {
    let ir = retrying_ir();
    let written = ir
        .bindings()
        .values()
        .flat_map(|binding| binding.mapping.iter())
        .find(|mapping| mapping.target == "price")
        .map(|mapping| match &mapping.value {
            ResolvedMappingValue::Literal { value } => value.clone(),
            other => panic!("price is not a literal: {other:?}"),
        })
        .expect("price is mapped");
    let router = Router::from_ir(&ir).expect("routable");
    let command: Command = router
        .route("receive")
        .expect("receive is routed")
        .map(&event("retrying.core.Arrived", json!({ "note": "hi" })))
        .expect("the event maps");
    assert_eq!(
        command.fields["price"],
        Value::String(written),
        "ess-gen types a Decimal as a decimal string, and its AsyncAPI binding constant is that string"
    );
}

struct FailFirstDelivery<C> {
    inner: C,
    failed: bool,
}

impl<C: Channel> Channel for FailFirstDelivery<C> {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        if matches!(frame, Frame::Deliver(_)) && !self.failed {
            self.failed = true;
            return Err(ChannelError::Closed);
        }
        self.inner.send(frame)
    }

    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError> {
        self.inner.receive(wait)
    }
}

#[test]
fn at_most_once_with_on_failure_retry_attempts_again_after_a_failed_send() {
    let router = Router::from_ir(&retrying_ir()).expect("routable");
    let (near, far) = in_process::pair();
    let mut source = Sender::new("retrying-service", router)
        .expect("retrying-service exists")
        .with_acknowledgement_timeout(Duration::from_millis(10));
    source.connect(
        "retrying-service",
        FailFirstDelivery {
            inner: near,
            failed: false,
        },
    );
    let mut sink = Receiver::new("retrying-service", far);

    source
        .publish(
            1,
            event("retrying.core.Arrived", json!({ "note": "again" })),
        )
        .expect("staged");
    source.mark_durable(1);
    assert!(
        source.flush().is_err(),
        "the first attempt does not land, and says so"
    );

    let later = Instant::now() + Duration::from_secs(60);
    let _ = source.flush();
    let _ = source.redeliver(later);
    let arrived: Option<Envelope> = sink.receive(QUIET).expect("open");
    assert!(
        arrived.is_some(),
        "on_failure: retry puts the attempt back when an at_most_once send did not land"
    );
}

#[test]
fn a_tcp_send_to_a_peer_that_stopped_reading_returns() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let mut near = TcpChannel::connect(listener.local_addr().expect("bound")).expect("connects");
    let (stalled, _) = listener.accept().expect("accepts");

    let big = "x".repeat(1024 * 1024);
    let (done, outcome) = mpsc::channel();
    std::thread::spawn(move || {
        for n in 0..64_u64 {
            let frame = Frame::Deliver(Envelope {
                id: DeliveryId(n),
                binding: "b".to_owned(),
                from: "f".to_owned(),
                to: "t".to_owned(),
                delivery: ess_domain::binding::Delivery::AtMostOnce,
                on_failure: ess_domain::binding::Failure::Drop,
                attempt: 1,
                command: Command {
                    name: "c".to_owned(),
                    fields: object(json!({ "body": big })),
                },
            });
            if near.send(frame).is_err() {
                break;
            }
        }
        let _ = done.send(());
    });

    let returned = outcome.recv_timeout(Duration::from_secs(10)).is_ok();
    drop(stalled);
    assert!(
        returned,
        "64 MiB to a peer that reads nothing: the TCP send blocked for 10 s; the in-process send never blocks"
    );
}

fn raw_pair() -> (TcpStream, TcpChannel) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let raw = TcpStream::connect(listener.local_addr().expect("bound")).expect("connects");
    let (accepted, _) = listener.accept().expect("accepts");
    (raw, TcpChannel::new(accepted).expect("a channel"))
}

fn framed(body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(body.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(body);
    bytes
}

#[test]
fn probe_a_malformed_frame_is_refused_and_the_next_one_is_read() {
    let (mut raw, mut channel) = raw_pair();
    raw.write_all(&framed(b"{not json")).expect("writes");
    raw.write_all(&framed(br#"{"acknowledge":5}"#))
        .expect("writes");
    assert!(matches!(
        channel.receive(PATIENCE),
        Err(ChannelError::Frame(_))
    ));
    assert_eq!(
        channel.receive(PATIENCE).expect("open"),
        Some(Frame::Acknowledge(DeliveryId(5)))
    );
}

#[test]
fn probe_a_prefix_exactly_at_the_cap_waits_for_its_body() {
    let (mut raw, mut channel) = raw_pair();
    let size = u32::try_from(MAX_FRAME).expect("fits");
    raw.write_all(&size.to_be_bytes()).expect("writes");
    raw.write_all(b"{").expect("writes");
    assert!(channel.receive(QUIET).expect("not refused").is_none());
    let mut sink = [0_u8; 1];
    raw.set_nonblocking(true).expect("nonblocking");
    assert!(raw.read(&mut sink).is_err(), "nothing is sent back");
}

#[test]
fn probe_an_acknowledgement_for_another_id_or_a_second_one_settles_nothing_extra() {
    let (near, far) = in_process::pair();
    let mut source = Sender::new("fan-source", fan_router())
        .expect("fan-source exists")
        .with_acknowledgement_timeout(Duration::from_secs(2));
    let (to_beta, _beta_end) = in_process::pair();
    source.connect("a-sink", near);
    source.connect("b-sink", to_beta);
    let mut alpha = Receiver::new("a-sink", far);

    source.publish(1, shouted("hi")).expect("staged");
    source.mark_durable(1);
    source.flush().expect("sent");
    let first = alpha.receive(PATIENCE).expect("open").expect("arrives");

    let mut forged = first.clone();
    forged.id = DeliveryId(first.id.0 + 100);
    alpha.acknowledge(&forged).expect("sent");
    assert_eq!(source.collect_acknowledgements(QUIET).expect("open"), 0);
    assert_eq!(source.unacknowledged(), 2);

    assert_eq!(
        source
            .redeliver(Instant::now() + Duration::from_secs(3))
            .expect("open"),
        2
    );
    let second = alpha.receive(PATIENCE).expect("open").expect("redelivered");
    assert_eq!(second.id, first.id);
    alpha.acknowledge(&first).expect("sent");
    alpha.acknowledge(&second).expect("sent");
    let deadline = Instant::now() + PATIENCE;
    while source.unacknowledged() > 1 && Instant::now() < deadline {
        source.collect_acknowledgements(QUIET).expect("open");
    }
    assert_eq!(source.unacknowledged(), 1);
}

#[test]
fn probe_out_of_order_and_repeated_durable_marks_release_nothing_early() {
    let (near, far) = in_process::pair();
    let (to_beta, _beta_end) = in_process::pair();
    let mut source = Sender::new("fan-source", fan_router()).expect("fan-source exists");
    source.connect("a-sink", near);
    source.connect("b-sink", to_beta);
    let mut alpha = Receiver::new("a-sink", far);

    source.publish(3, shouted("three")).expect("staged");
    source.publish(1, shouted("one")).expect("staged");
    source.mark_durable(1);
    source.mark_durable(1);
    source.mark_durable(0);
    assert_eq!(source.flush().expect("sent"), 2);
    let heard = alpha.receive(PATIENCE).expect("open").expect("one arrives");
    assert_eq!(heard.command.fields["word"], json!("one"));
    assert!(alpha.receive(QUIET).expect("open").is_none());
    assert_eq!(source.staged(), 2);
}
