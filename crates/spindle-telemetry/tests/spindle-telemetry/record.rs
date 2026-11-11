//! Recording around the kernel and the store, into the default sink.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, Store};
use spindle_telemetry::{
    Counters, CountingStore, DURATION_BOUNDS_US, Record, Sink, Telemetry, catalogue,
};

fn rally() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn table(ir: &spindle_spec::EssIr) -> Arc<Counters> {
    Arc::new(Counters::new(
        catalogue(ir, "rally-table").expect("rally-table exists"),
    ))
}

#[test]
fn executing_a_command_increments_its_outcome_counter() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let ir = &loaded.ir;
    let counters = table(ir);
    let sink = Sink::new(counters.clone());
    let mut store = CountingStore::new(MemoryStore::default(), sink.clone());

    let command = "rally.table.OpenMatch";
    let input = object(json!({"match_id": "m1", "target_score": 2}));
    let started = Instant::now();
    let result = spindle_kernel::execute(ir, &mut store, &mut NoExternals, command, &input);
    sink.executed(command, &result, started.elapsed());
    let executed = result.expect("the match opens");
    assert_eq!(executed.outcome, "opened");
    assert_eq!(
        counters.value("command.rally.table.OpenMatch.opened"),
        Some(1)
    );
    let duration = counters
        .histogram("command.rally.table.OpenMatch.duration_us")
        .expect("a duration histogram");
    assert_eq!(duration.counts.iter().sum::<u64>(), 1);
    assert_eq!(duration.bounds, DURATION_BOUNDS_US.to_vec());

    assert!(!executed.events.is_empty(), "opening a match emits events");
    for event in &executed.events {
        assert_eq!(
            counters.value(&format!("event.{}.appended", event.name)),
            Some(
                executed
                    .events
                    .iter()
                    .filter(|other| other.name == event.name)
                    .count() as u64
            ),
            "{}",
            event.name
        );
    }
    assert_eq!(counters.value("store.rows_written"), Some(1));
    let mut bytes = 0;
    let stored = store
        .load("rally.table.Match", &json!("m1"))
        .expect("the store reads")
        .expect("the match is stored");
    bytes += serde_json::to_vec(&stored.row).expect("a row").len() as u64;
    for event in store.inner().events() {
        bytes += serde_json::to_vec(&event).expect("an event").len() as u64;
    }
    assert_eq!(counters.value("store.bytes_written"), Some(bytes));

    let again = spindle_kernel::execute(ir, &mut store, &mut NoExternals, command, &input);
    sink.executed(command, &again, Duration::from_micros(3));
    let refused = counters
        .catalogue()
        .counters
        .iter()
        .filter(|counter| counter.name.starts_with("command.rally.table.OpenMatch."))
        .filter_map(|counter| counters.value(&counter.name))
        .sum::<u64>();
    assert_eq!(refused, 2, "both runs are counted once each");
    assert_eq!(
        counters.value("store.rows_written"),
        Some(1),
        "nothing more was written"
    );

    let invalid = spindle_kernel::execute(
        ir,
        &mut store,
        &mut NoExternals,
        command,
        &object(json!({"match_id": 7})),
    );
    sink.executed(command, &invalid, Duration::ZERO);
    assert_eq!(
        counters.value("command.rally.table.OpenMatch.failed.invalid_input"),
        Some(1),
        "{invalid:?}"
    );
    assert_eq!(counters.uncatalogued(), 0);
}

#[test]
fn a_name_the_catalogue_lacks_is_counted_uncatalogued() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let counters = table(&loaded.ir);
    let sink = Sink::new(counters.clone());
    let unknown = spindle_kernel::execute(
        &loaded.ir,
        &mut MemoryStore::default(),
        &mut NoExternals,
        "rally.table.Nowhere",
        &Map::new(),
    );
    sink.executed("rally.table.Nowhere", &unknown, Duration::ZERO);
    counters.record(Record::Appended {
        event: "rally.west.BallSeen",
    });
    sink.carried("nowhere", spindle_telemetry::Carried::Sent);
    assert_eq!(counters.uncatalogued(), 3);
    assert!(
        counters
            .snapshot()
            .counters
            .iter()
            .all(|counter| counter.value == 0)
    );
}

#[test]
fn durations_fall_into_fixed_buckets() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let counters = table(&loaded.ir);
    let outcome = spindle_telemetry::Outcome::Declared("opened");
    for micros in [0, 10, 11, 1_000_000, 1_000_001, u64::MAX] {
        counters.record(Record::Command {
            command: "rally.table.OpenMatch",
            outcome,
            elapsed: Duration::from_micros(micros),
        });
    }
    let histogram = counters
        .histogram("command.rally.table.OpenMatch.duration_us")
        .expect("a histogram");
    let mut expected = vec![0; DURATION_BOUNDS_US.len() + 1];
    expected[0] = 2;
    expected[1] = 1;
    expected[DURATION_BOUNDS_US.len() - 1] = 1;
    expected[DURATION_BOUNDS_US.len()] = 2;
    assert_eq!(histogram.counts, expected);
    assert_eq!(
        counters.value("command.rally.table.OpenMatch.opened"),
        Some(6)
    );
}

#[test]
fn the_default_sink_records_nothing() {
    let sink = Sink::default();
    sink.carried("west-watches", spindle_telemetry::Carried::Sent);
    assert_eq!(format!("{sink:?}"), "Sink(none)");
}
