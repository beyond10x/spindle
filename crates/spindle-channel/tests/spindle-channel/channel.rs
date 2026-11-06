use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ess_domain::binding::{Delivery, Failure};
use serde_json::{Map, Value, json};
use spindle_channel::{
    Channel, ChannelError, Command, Envelope, Event, Frame, MappingError, Receiver, RouteError,
    Router, Sender, Unmet, in_process, tcp::TcpChannel,
};

const PATIENCE: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(50);

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn relay() -> PathBuf {
    manifest().join("../spindle-capability/tests/fixtures/relay")
}

fn rally() -> PathBuf {
    manifest().join("../../examples/rally/ess")
}

fn library() -> PathBuf {
    manifest().join("tests/fixtures/library")
}

fn loan_closed(member: &str) -> Event {
    event(
        "library.lending.LoanClosed",
        json!({ "loan_id": "loan-1", "member_id": member }),
    )
}

fn router(path: &Path) -> Router {
    let loaded = spindle_spec::load(path).expect("the specification compiles");
    Router::from_ir(&loaded.ir).expect("every event binding is routable")
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

fn pulsed(beat: &str) -> Event {
    event("relay.source.Pulsed", json!({ "beat": beat }))
}

fn ball_moved() -> Event {
    event(
        "rally.table.BallMoved",
        json!({
            "match_id": "m-1",
            "ball_x": 4,
            "ball_y": 2,
            "heading_x": "East",
            "heading_y": "Up",
            "west_paddle_y": 1,
            "east_paddle_y": 3,
        }),
    )
}

fn single(identity: &str, text: &str) -> Router {
    let ir = spindle_spec::compile(&[identity.to_owned()], &[text.to_owned()])
        .expect("the inline specification compiles");
    Router::from_ir(&ir).expect("every event binding is routable")
}

fn relay_sender(router: Router) -> Sender {
    Sender::new("relay-source", router)
        .expect("relay-source is a component")
        .with_acknowledgement_timeout(Duration::from_secs(2))
}

fn relay_receiver(channel: impl Channel + 'static) -> Receiver {
    Receiver::new("relay-sink", channel)
}

#[test]
fn an_event_is_mapped_to_the_bound_command_input() {
    let rally = router(&rally());

    let west = rally.route("west-watches").expect("west-watches is routed");
    assert_eq!(west.event, "rally.table.BallMoved");
    assert_eq!(west.command, "rally.west.ObserveBall");
    assert_eq!(west.from, "rally-table");
    assert_eq!(west.to, "rally-player-west");
    assert_eq!(west.delivery, Delivery::AtMostOnce);
    assert_eq!(west.on_failure, Failure::Drop);
    assert_eq!(
        west.map(&ball_moved()).expect("the event maps"),
        Command {
            name: "rally.west.ObserveBall".to_owned(),
            fields: object(json!({ "match_id": "m-1", "ball_x": 4, "ball_y": 2, "paddle_y": 1 })),
        }
    );

    let east = rally.route("east-watches").expect("east-watches is routed");
    assert_eq!(
        east.map(&ball_moved()).expect("the event maps").fields,
        object(json!({ "match_id": "m-1", "ball_x": 4, "ball_y": 2, "paddle_y": 3 }))
    );

    let both = rally.map(&ball_moved()).expect("the event maps");
    let mut bound = both
        .iter()
        .map(|(route, command)| (route.binding.as_str(), command.name.as_str()))
        .collect::<Vec<_>>();
    bound.sort_unstable();
    assert_eq!(
        bound,
        [
            ("east-watches", "rally.east.ObserveBall"),
            ("west-watches", "rally.west.ObserveBall"),
        ]
    );

    assert!(
        rally.route("tick").is_none(),
        "a periodic binding is the timer's, not the channel's"
    );

    let relay = router(&relay());
    let hear = relay.route("hear-pulse").expect("hear-pulse is routed");
    assert_eq!(
        (hear.from.as_str(), hear.to.as_str()),
        ("relay-source", "relay-sink")
    );
    assert_eq!(hear.delivery, Delivery::AtLeastOnce);
    assert_eq!(
        hear.map(&pulsed("one")).expect("the event maps"),
        Command {
            name: "relay.sink.Hear".to_owned(),
            fields: object(json!({ "beat": "one" })),
        }
    );

    let missing = hear
        .map(&event("relay.source.Pulsed", json!({})))
        .expect_err("a required source field is absent");
    assert_eq!(
        missing,
        MappingError::MissingField {
            binding: "hear-pulse".to_owned(),
            field: "beat".to_owned(),
        }
    );
}

const LITERALS: &str = r"
format: ess/34
system: literal
version: v1
domain: literal.core
events:
  - name: literal.core.Arrived
    fields:
      - {name: note, type: String}
  - name: literal.core.Done
    fields:
      - {name: note, type: String}
commands:
  - name: literal.core.Publish
    input:
      - {name: note, type: String}
    outcomes:
      - name: accepted
        emits: [literal.core.Arrived]
        payload:
          literal.core.Arrived: {note: input.note}
  - name: literal.core.Receive
    input:
      - {name: note, type: String}
      - {name: label, type: String}
      - {name: count, type: Integer}
      - {name: flag, type: Boolean}
    outcomes:
      - name: accepted
        emits: [literal.core.Done]
        payload:
          literal.core.Done: {note: input.note}
components:
  - component: literal-service
    owns:
      domains: [literal.core]
    accepts:
      commands: [literal.core.Publish, literal.core.Receive]
    publishes:
      events: [literal.core.Arrived, literal.core.Done]
bindings:
  - id: receive
    when: {event: literal.core.Arrived}
    invoke: {command: literal.core.Receive}
    mapping:
      note: event.note
      label: fixed
      count: 3
      flag: true
    delivery: at_most_once
    on_failure: drop
";

#[test]
fn a_literal_is_mapped_as_its_target_type() {
    let router = single("literal.yaml", LITERALS);
    let route = router.route("receive").expect("receive is routed");
    assert_eq!(
        route
            .map(&event("literal.core.Arrived", json!({ "note": "hi" })))
            .expect("the event maps")
            .fields,
        object(json!({ "note": "hi", "label": "fixed", "count": 3, "flag": true }))
    );
}

const ACCESSOR: &str = r"
format: ess/34
system: accessor
version: v1
domain: accessor.core
types:
  - name: accessor.core.Body
    kind: struct
    fields:
      - {name: status, type: String}
events:
  - name: accessor.core.Arrived
    fields:
      - {name: data, type: accessor.core.Body}
  - name: accessor.core.Done
    fields:
      - {name: result, type: String}
commands:
  - name: accessor.core.Publish
    input:
      - {name: data, type: accessor.core.Body}
    outcomes:
      - name: accepted
        emits: [accessor.core.Arrived]
        payload:
          accessor.core.Arrived: {data: input.data}
  - name: accessor.core.Receive
    input:
      - {name: text, type: String}
    outcomes:
      - name: accepted
        emits: [accessor.core.Done]
        payload:
          accessor.core.Done: {result: input.text}
components:
  - component: accessor-service
    owns:
      domains: [accessor.core]
    accepts:
      commands: [accessor.core.Publish, accessor.core.Receive]
    publishes:
      events: [accessor.core.Arrived, accessor.core.Done]
bindings:
  - id: project
    when: {event: accessor.core.Arrived}
    invoke: {command: accessor.core.Receive}
    mapping:
      text: event.data.status
    delivery: at_least_once
    on_failure: drop
";

#[test]
fn a_mapping_source_the_channel_does_not_carry_is_unsupported() {
    let ir = spindle_spec::compile(&["accessor.yaml".to_owned()], &[ACCESSOR.to_owned()])
        .expect("the inline specification compiles");
    let refused = Router::from_ir(&ir).expect_err("an event accessor is not carried");
    assert!(
        matches!(
            &refused,
            RouteError::Unsupported { binding, construct }
                if binding == "project" && construct.contains("event.data.status")
        ),
        "{refused:?}"
    );
}

#[test]
fn an_event_reaches_the_bound_component_in_process() {
    let (table_to_west, west_end) = in_process::pair();
    let (table_to_east, east_end) = in_process::pair();
    let mut table = Sender::new("rally-table", router(&rally())).expect("rally-table exists");
    table.connect("rally-player-west", table_to_west);
    table.connect("rally-player-east", table_to_east);
    let mut west = Receiver::new("rally-player-west", west_end);
    let mut east = Receiver::new("rally-player-east", east_end);

    assert_eq!(table.publish(1, ball_moved()).expect("staged"), 2);
    table.mark_durable(1);
    assert_eq!(table.flush().expect("sent"), 2);

    let seen = west
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("west receives");
    assert_eq!(seen.binding, "west-watches");
    assert_eq!(seen.to, "rally-player-west");
    assert_eq!(seen.command.name, "rally.west.ObserveBall");
    assert_eq!(seen.command.fields["paddle_y"], json!(1));

    let seen = east
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("east receives");
    assert_eq!(seen.command.name, "rally.east.ObserveBall");
    assert_eq!(seen.command.fields["paddle_y"], json!(3));

    assert_eq!(
        table.unacknowledged(),
        0,
        "at most once keeps nothing to redeliver"
    );
    assert!(west.receive(QUIET).expect("the channel is open").is_none());
}

#[test]
fn an_event_reaches_the_bound_component_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let address = listener.local_addr().expect("the bound address");
    let near = TcpChannel::connect(address).expect("the source connects");
    let (stream, _) = listener.accept().expect("the sink accepts");
    let far = TcpChannel::new(stream).expect("the accepted stream is a channel");

    let mut source = relay_sender(router(&relay()));
    source.connect("relay-sink", near);
    let mut sink = relay_receiver(far);

    assert_eq!(
        source.publish(7, pulsed("over-the-wire")).expect("staged"),
        1
    );
    source.mark_durable(7);
    assert_eq!(source.flush().expect("sent"), 1);

    let heard = sink
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("the sink receives");
    assert_eq!(
        heard.command,
        Command {
            name: "relay.sink.Hear".to_owned(),
            fields: object(json!({ "beat": "over-the-wire" })),
        }
    );
    assert_eq!(heard.binding, "hear-pulse");
    assert_eq!(heard.from, "relay-source");
    assert_eq!(heard.delivery, Delivery::AtLeastOnce);
    assert_eq!(source.unacknowledged(), 1);

    sink.acknowledge(&heard)
        .expect("the acknowledgement is sent");
    let deadline = Instant::now() + PATIENCE;
    while source.unacknowledged() > 0 && Instant::now() < deadline {
        source
            .collect_acknowledgements(QUIET)
            .expect("the channel is open");
    }
    assert_eq!(source.unacknowledged(), 0);
}

struct LoseFirstAcknowledgement<C> {
    inner: C,
    lost: usize,
}

impl<C: Channel> Channel for LoseFirstAcknowledgement<C> {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        if matches!(frame, Frame::Acknowledge(_)) && self.lost == 0 {
            self.lost += 1;
            return Ok(());
        }
        self.inner.send(frame)
    }

    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError> {
        self.inner.receive(wait)
    }
}

#[test]
fn at_least_once_redelivers_after_a_lost_acknowledgement() {
    let (near, far) = in_process::pair();
    let mut source = relay_sender(router(&relay()));
    source.connect("relay-sink", near);
    let mut sink = relay_receiver(LoseFirstAcknowledgement {
        inner: far,
        lost: 0,
    });

    source.publish(1, pulsed("one")).expect("staged");
    source.mark_durable(1);
    assert_eq!(source.flush().expect("sent"), 1);

    let first: Envelope = sink
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("first delivery");
    assert_eq!(first.attempt, 1);
    sink.acknowledge(&first).expect("the loss is silent");

    assert_eq!(source.collect_acknowledgements(QUIET).expect("open"), 0);
    assert_eq!(source.unacknowledged(), 1);
    assert_eq!(
        source.redeliver(Instant::now()).expect("open"),
        0,
        "not before the acknowledgement is overdue"
    );
    assert!(sink.receive(QUIET).expect("open").is_none());

    let overdue = Instant::now() + Duration::from_secs(3);
    assert_eq!(source.redeliver(overdue).expect("open"), 1);
    let second = sink
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("redelivery");
    assert_eq!(second.id, first.id);
    assert_eq!(second.command, first.command);
    assert_eq!(second.attempt, 2);

    sink.acknowledge(&second).expect("sent");
    let deadline = Instant::now() + PATIENCE;
    while source.unacknowledged() > 0 && Instant::now() < deadline {
        source.collect_acknowledgements(QUIET).expect("open");
    }
    assert_eq!(source.unacknowledged(), 0);
    assert_eq!(
        source
            .redeliver(overdue + Duration::from_secs(60))
            .expect("open"),
        0
    );
    assert!(sink.receive(QUIET).expect("open").is_none());
}

#[test]
fn nothing_leaves_the_outbox_before_the_state_is_durable() {
    let (near, far) = in_process::pair();
    let mut source = relay_sender(router(&relay()));
    source.connect("relay-sink", near);
    let mut sink = relay_receiver(far);

    source.publish(1, pulsed("one")).expect("staged");
    assert_eq!(source.flush().expect("nothing to send"), 0);
    assert_eq!(source.staged(), 1);
    assert!(sink.receive(QUIET).expect("open").is_none());

    source.publish(2, pulsed("two")).expect("staged");
    source.mark_durable(1);
    assert_eq!(source.flush().expect("sent"), 1);
    assert_eq!(source.staged(), 1);
    let heard = sink.receive(PATIENCE).expect("open").expect("one arrives");
    assert_eq!(heard.command.fields["beat"], json!("one"));
    assert!(sink.receive(QUIET).expect("open").is_none());

    source.mark_durable(2);
    assert_eq!(source.flush().expect("sent"), 1);
    assert_eq!(source.staged(), 0);
    let heard = sink.receive(PATIENCE).expect("open").expect("two arrives");
    assert_eq!(heard.command.fields["beat"], json!("two"));
}

#[test]
fn a_component_publishes_only_its_own_events() {
    let (near, _far) = in_process::pair();
    let mut sink = Sender::new("relay-sink", router(&relay())).expect("relay-sink exists");
    sink.connect("relay-source", near);
    let refused = sink
        .publish(1, pulsed("one"))
        .expect_err("relay-sink does not publish Pulsed");
    assert!(
        matches!(&refused, ChannelError::NotPublished { component, event }
            if component == "relay-sink" && event == "relay.source.Pulsed"),
        "{refused:?}"
    );
    assert_eq!(sink.staged(), 0);
}

#[test]
fn dropping_retries_carries_a_retry_binding_as_drop() {
    let retried = router(&manifest().join("../spindle-cli/tests/fixtures/retried"));
    let route = retried.route("index-note").expect("the binding is routed");
    assert_eq!(route.on_failure, Failure::Retry);
    let dropped = retried.dropping_retries();
    let route = dropped.route("index-note").expect("the binding is routed");
    assert_eq!(route.on_failure, Failure::Drop);
    let kept = router(&relay()).dropping_retries();
    assert_eq!(
        kept.routes()
            .iter()
            .map(|route| route.on_failure)
            .collect::<Vec<_>>(),
        router(&relay())
            .routes()
            .iter()
            .map(|route| route.on_failure)
            .collect::<Vec<_>>(),
        "a binding that does not retry keeps its policy"
    );
}

/// `reader_id: event.member_id` crosses the declared `MemberId -> ReaderRef` conversion: the
/// value is carried unchanged, as ESS 0.86.0's interpreter carries it (`interpret/bindings.rs`,
/// `mapped`), and reaches the component that accepts `SendThanks`.
#[test]
fn a_mapping_across_a_conversion_carries_the_value() {
    let library = router(&library());
    let thanks = library
        .route("thank-on-close")
        .expect("a mapping across a declared conversion is routed");
    assert_eq!(
        (thanks.from.as_str(), thanks.to.as_str()),
        ("lending", "notices")
    );
    let thanked = Command {
        name: "library.notices.SendThanks".to_owned(),
        fields: object(json!({ "reader_id": "m-7" })),
    };
    assert_eq!(
        thanks.map(&loan_closed("m-7")).expect("the event maps"),
        thanked
    );

    let (near, far) = in_process::pair();
    let mut lending = Sender::new("lending", library).expect("lending is a component");
    lending.connect("notices", near);
    let mut notices = Receiver::new("notices", far);
    assert_eq!(lending.publish(1, loan_closed("m-7")).expect("staged"), 1);
    lending.mark_durable(1);
    assert_eq!(lending.flush().expect("sent"), 1);
    let seen = notices
        .receive(PATIENCE)
        .expect("the channel is open")
        .expect("notices receives");
    assert_eq!(seen.binding, "thank-on-close");
    assert_eq!(seen.command, thanked);
    assert!(lending.take_unmet().is_empty());
}

/// `member-0042` is a `MemberId` and no `ReaderRef` (`value.count <= 8`). ESS checks a mapped
/// value against the input's declared type before the command port (`validate_typed_value`), so
/// the binding's input is unmet: no envelope is staged or sent, the sender reports the binding, and
/// the publish itself still succeeds.
#[test]
fn a_value_the_target_type_refuses_is_an_unmet_input() {
    let library = router(&library());
    let thanks = library
        .route("thank-on-close")
        .expect("a mapping across a declared conversion is routed");
    let refused = thanks
        .map(&loan_closed("member-0042"))
        .expect_err("the value is no ReaderRef");
    assert!(
        matches!(&refused, MappingError::Unmet { binding, target, .. }
            if binding == "thank-on-close" && target == "reader_id"),
        "{refused:?}"
    );

    let (near, far) = in_process::pair();
    let mut lending = Sender::new("lending", library).expect("lending is a component");
    lending.connect("notices", near);
    let mut notices = Receiver::new("notices", far);
    assert_eq!(
        lending
            .publish(1, loan_closed("member-0042"))
            .expect("an unmet binding input does not refuse the publish"),
        0
    );
    lending.mark_durable(1);
    assert_eq!(lending.flush().expect("nothing to send"), 0);
    assert!(notices.receive(QUIET).expect("open").is_none());
    assert_eq!(
        lending.take_unmet(),
        [Unmet {
            binding: "thank-on-close".to_owned(),
            to: "notices".to_owned(),
            command: "library.notices.SendThanks".to_owned(),
            error: refused,
        }]
    );
    assert!(lending.take_unmet().is_empty(), "each is reported once");
}
