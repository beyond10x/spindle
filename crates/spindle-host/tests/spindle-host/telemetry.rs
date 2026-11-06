//! Each component's counters while rally plays in one process.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, ManualClock};

const TICKS: u64 = 20;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn a_played_match_is_counted_on_every_component() {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    let environment = Environment::parse(&text).expect("an environment");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host = Host::new(loaded.ir, &environment, clock.clone(), state.path())
        .expect("laptop admits rally");
    let opened = host
        .submit(
            "rally.table.OpenMatch",
            object(json!({"match_id": "m1", "target_score": 99})),
        )
        .expect("the match opens");
    host.start("tick", object(json!({"match_id": "m1"})))
        .expect("the tick starts");

    let mut ticked = 0;
    let mut observed = 0;
    let mut reports = vec![opened];
    for _ in 0..TICKS {
        clock.advance(Duration::from_secs(1));
        reports.push(host.advance().expect("the host advances"));
    }
    for report in &reports {
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        ticked += report
            .ran
            .iter()
            .filter(|ran| matches!(ran.cause, Cause::Tick { .. }))
            .count() as u64;
        observed += report
            .ran
            .iter()
            .filter(|ran| ran.command == "rally.west.ObserveBall")
            .count() as u64
            + report
                .refused
                .iter()
                .filter(|refused| refused.command == "rally.west.ObserveBall")
                .count() as u64;
    }
    assert_eq!(ticked, TICKS);

    let table = host.counters("rally-table").expect("the table runs");
    let west = host.counters("rally-player-west").expect("west runs");
    assert_eq!(table.value("timer.tick.fired"), Some(TICKS));
    assert_eq!(table.value("timer.tick.late"), Some(0));
    assert_eq!(table.value("command.rally.table.OpenMatch.opened"), Some(1));
    let tick_outcomes = table
        .catalogue()
        .counters
        .iter()
        .filter(|counter| counter.name.starts_with("command.rally.table.Tick."))
        .filter_map(|counter| table.value(&counter.name))
        .sum::<u64>();
    assert_eq!(tick_outcomes, TICKS, "every tick is one Tick outcome");
    let moved = table
        .value("event.rally.table.BallMoved.appended")
        .expect("catalogued");
    assert!(moved > 0);
    assert_eq!(table.value("binding.west-watches.sent"), Some(moved));
    assert_eq!(west.value("binding.west-watches.delivered"), Some(moved));
    assert_eq!(
        observed, moved,
        "each delivery ran ObserveBall once, taken or refused"
    );
    assert!(table.value("store.rows_written") > Some(TICKS));
    assert!(table.value("store.bytes_written") > table.value("store.rows_written"));
    assert_eq!(
        west.value("binding.west-moves.sent"),
        table.value("binding.west-moves.delivered")
    );
    for component in ["rally-table", "rally-player-west", "rally-player-east"] {
        let counters = host.counters(component).expect("it runs");
        assert_eq!(counters.uncatalogued(), 0, "{component}");
    }
    assert!(host.counters("nowhere").is_none());
}
