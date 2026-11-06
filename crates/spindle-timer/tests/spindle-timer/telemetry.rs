//! The timer records each tick fired, late and dropped, by binding.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use spindle_telemetry::{Record, Sink, Telemetry};
use spindle_timer::{Clock, ManualClock, Timer};

#[derive(Default)]
struct Recorded(Mutex<BTreeMap<String, u64>>);

impl Telemetry for Recorded {
    fn record(&self, record: Record<'_>) {
        if let Record::Timed {
            binding,
            timed,
            count,
        } = record
        {
            let mut counts = self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *counts
                .entry(format!("timer.{binding}.{}", timed.as_str()))
                .or_default() += count;
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

#[test]
fn ticks_are_counted_fired_late_and_dropped() {
    let rally = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess");
    let loaded = spindle_spec::load(&rally).expect("rally compiles");
    let recorded = Arc::new(Recorded::default());
    let clock = ManualClock::new();
    let mut timer = Timer::activate(&loaded.ir, &clock).with_telemetry(Sink::new(recorded.clone()));

    clock.advance(Duration::from_secs(1));
    let ticks = timer.due(clock.now());
    assert_eq!(ticks.len(), 1);
    timer.complete(&ticks[0]).expect("the open tick");
    assert_eq!(recorded.counts(), counts(&[("timer.tick.fired", 1)]));

    clock.advance(Duration::from_millis(3_500));
    let ticks = timer.due(clock.now());
    assert_eq!(ticks.len(), 1, "one pending tick, the earliest");
    assert_eq!(timer.dropped(&ticks[0].binding), Some(2));
    timer.complete(&ticks[0]).expect("the open tick");
    assert_eq!(
        recorded.counts(),
        counts(&[
            ("timer.tick.dropped", 2),
            ("timer.tick.fired", 2),
            ("timer.tick.late", 1),
        ]),
        "the pending tick of second 2 is yielded at 4.5 s, past its successor's deadline"
    );

    clock.advance(Duration::from_millis(500));
    let ticks = timer.due(clock.now());
    assert_eq!(ticks.len(), 1);
    assert_eq!(
        recorded.counts(),
        counts(&[
            ("timer.tick.dropped", 2),
            ("timer.tick.fired", 3),
            ("timer.tick.late", 1),
        ]),
        "the tick of second 5 is on time"
    );
}
