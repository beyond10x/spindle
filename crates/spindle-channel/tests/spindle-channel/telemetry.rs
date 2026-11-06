//! The endpoints record each envelope sent, delivered, redelivered and dropped, by binding.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_channel::{Channel, ChannelError, Event, Frame, Receiver, Router, Sender, in_process};
use spindle_telemetry::{Record, Sink, Telemetry};

const PATIENCE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Recorded(Mutex<BTreeMap<String, u64>>);

impl Telemetry for Recorded {
    fn record(&self, record: Record<'_>) {
        if let Record::Carried { binding, carried } = record {
            let mut counts = self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *counts
                .entry(format!("binding.{binding}.{}", carried.as_str()))
                .or_default() += 1;
        }
    }
}

impl Recorded {
    fn counts(&self) -> BTreeMap<String, u64> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

fn counts(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
    pairs
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect()
}

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn router(path: PathBuf) -> Router {
    let loaded = spindle_spec::load(&path).expect("the specification compiles");
    Router::from_ir(&loaded.ir).expect("every event binding is routable")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn ball_moved() -> Event {
    Event {
        name: "rally.table.BallMoved".to_owned(),
        fields: object(json!({
            "match_id": "m-1",
            "ball_x": 4,
            "ball_y": 2,
            "heading_x": "East",
            "heading_y": "Up",
            "west_paddle_y": 1,
            "east_paddle_y": 3,
        })),
    }
}

fn pulsed() -> Event {
    Event {
        name: "relay.source.Pulsed".to_owned(),
        fields: object(json!({"beat": "one"})),
    }
}

#[test]
fn sending_and_delivering_are_counted_by_binding() {
    let sent = Arc::new(Recorded::default());
    let delivered = Arc::new(Recorded::default());
    let (near, far) = in_process::pair();
    let (east_near, _east_far) = in_process::pair();
    let mut table = Sender::new(
        "rally-table",
        router(manifest().join("../../examples/rally/ess")),
    )
    .expect("rally-table exists")
    .with_telemetry(Sink::new(sent.clone()));
    table.connect("rally-player-west", near);
    table.connect("rally-player-east", east_near);
    let mut west =
        Receiver::new("rally-player-west", far).with_telemetry(Sink::new(delivered.clone()));

    table.publish(1, ball_moved()).expect("staged");
    table.publish(1, ball_moved()).expect("staged");
    table.mark_durable(1);
    assert_eq!(table.flush().expect("sent"), 4);
    assert_eq!(
        sent.counts(),
        counts(&[
            ("binding.east-watches.sent", 2),
            ("binding.west-watches.sent", 2)
        ])
    );
    for _ in 0..2 {
        west.receive(PATIENCE)
            .expect("open")
            .expect("west receives");
    }
    assert_eq!(
        delivered.counts(),
        counts(&[("binding.west-watches.delivered", 2)])
    );
}

struct LoseAcknowledgements<C>(C);

impl<C: Channel> Channel for LoseAcknowledgements<C> {
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError> {
        if matches!(frame, Frame::Acknowledge(_)) {
            return Ok(());
        }
        self.0.send(frame)
    }

    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError> {
        self.0.receive(wait)
    }
}

#[test]
fn a_redelivery_is_counted_as_redelivered_not_sent() {
    let recorded = Arc::new(Recorded::default());
    let (near, far) = in_process::pair();
    let mut source = Sender::new(
        "relay-source",
        router(manifest().join("../spindle-capability/tests/fixtures/relay")),
    )
    .expect("relay-source exists")
    .with_acknowledgement_timeout(Duration::from_secs(1))
    .with_telemetry(Sink::new(recorded.clone()));
    source.connect("relay-sink", near);
    let mut sink = Receiver::new("relay-sink", LoseAcknowledgements(far));

    source.publish(1, pulsed()).expect("staged");
    source.mark_durable(1);
    assert_eq!(source.flush().expect("sent"), 1);
    let first = sink.receive(PATIENCE).expect("open").expect("delivered");
    sink.acknowledge(&first).expect("lost silently");
    let overdue = Instant::now() + Duration::from_secs(5);
    assert_eq!(source.redeliver(overdue).expect("open"), 1);
    assert_eq!(
        recorded.counts(),
        counts(&[
            ("binding.hear-pulse.redelivered", 1),
            ("binding.hear-pulse.sent", 1)
        ])
    );
}

#[test]
fn an_at_most_once_envelope_lost_on_send_is_counted_dropped() {
    let recorded = Arc::new(Recorded::default());
    let (near, far) = in_process::pair();
    let (east_near, east_far) = in_process::pair();
    let mut table = Sender::new(
        "rally-table",
        router(manifest().join("../../examples/rally/ess")),
    )
    .expect("rally-table exists")
    .with_telemetry(Sink::new(recorded.clone()));
    table.connect("rally-player-west", near);
    table.connect("rally-player-east", east_near);
    drop(far);

    table.publish(1, ball_moved()).expect("staged");
    table.mark_durable(1);
    assert!(table.flush().is_err(), "west is gone");
    assert_eq!(
        recorded.counts(),
        counts(&[
            ("binding.east-watches.sent", 1),
            ("binding.west-watches.dropped", 1)
        ])
    );
    drop(east_far);
}

#[test]
fn a_discarded_envelope_is_counted_dropped_not_delivered() {
    let recorded = Arc::new(Recorded::default());
    let (near, far) = in_process::pair();
    let (east_near, _east_far) = in_process::pair();
    let mut table = Sender::new(
        "rally-table",
        router(manifest().join("../../examples/rally/ess")),
    )
    .expect("rally-table exists");
    table.connect("rally-player-west", near);
    table.connect("rally-player-east", east_near);
    let mut west =
        Receiver::new("rally-player-west", far).with_telemetry(Sink::new(recorded.clone()));

    for _ in 0..3 {
        table.publish(1, ball_moved()).expect("staged");
    }
    table.mark_durable(1);
    table.flush().expect("sent");
    assert_eq!(west.discard().expect("open"), 3);
    assert_eq!(
        recorded.counts(),
        counts(&[("binding.west-watches.dropped", 3)])
    );
}
