//! Adversarial case for `Counters::record` (`crates/spindle-telemetry/src/counters.rs`): "A record
//! whose name the catalogue lacks is counted in `uncatalogued`". A command record of a catalogued
//! command with an outcome the IR does not declare is counted as uncatalogued and also lands in
//! the command's duration histogram, so the histogram's total exceeds the sum of the command's
//! outcome and failure counters, and one record is counted twice.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use spindle_telemetry::{Counters, Outcome, Record, Telemetry, catalogue};

fn rally() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rally/ess")
}

#[test]
fn a_duration_histograms_sum_is_the_total_of_the_observed_durations() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let counters = Arc::new(Counters::new(
        catalogue(&loaded.ir, "rally-table").expect("rally-table exists"),
    ));
    for micros in [20, 700, 3_000_000] {
        counters.record(Record::Command {
            command: "rally.table.OpenMatch",
            outcome: Outcome::Declared("opened"),
            elapsed: Duration::from_micros(micros),
        });
    }
    let name = "command.rally.table.OpenMatch.duration_us";
    let histogram = counters.histogram(name).expect("a duration histogram");
    assert_eq!(histogram.sum_us, 20 + 700 + 3_000_000);
    assert_eq!(histogram.counts.len(), histogram.bounds.len() + 1);
    let snapshot = counters.snapshot();
    let in_snapshot = snapshot
        .histograms
        .iter()
        .find(|histogram| histogram.name == name)
        .expect("the snapshot holds it");
    assert_eq!(in_snapshot, &histogram);
}

#[test]
fn an_uncatalogued_outcome_is_not_also_timed_as_the_command() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let counters = Arc::new(Counters::new(
        catalogue(&loaded.ir, "rally-table").expect("rally-table exists"),
    ));
    counters.record(Record::Command {
        command: "rally.table.OpenMatch",
        outcome: Outcome::Declared("no-such-outcome"),
        elapsed: Duration::from_micros(20),
    });
    assert_eq!(counters.uncatalogued(), 1, "the record is uncatalogued");
    let timed = counters
        .histogram("command.rally.table.OpenMatch.duration_us")
        .expect("a duration histogram")
        .counts
        .iter()
        .sum::<u64>();
    let counted = counters
        .snapshot()
        .counters
        .iter()
        .filter(|counter| counter.name.starts_with("command.rally.table.OpenMatch."))
        .map(|counter| counter.value)
        .sum::<u64>();
    assert_eq!(
        timed, counted,
        "the duration histogram counts {timed} executions, the outcome counters {counted}"
    );
}
