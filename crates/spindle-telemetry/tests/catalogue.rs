//! The catalogue is derived from the compiled IR.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use spindle_kernel::{ExecuteError, StoreError};
use spindle_telemetry::{Failure, Kind, Source, catalogue};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn rally() -> PathBuf {
    manifest().join("../../examples/rally/ess")
}

fn relay() -> PathBuf {
    manifest().join("../spindle-capability/tests/fixtures/relay")
}

fn names(ir: &spindle_spec::EssIr, component: &str) -> BTreeSet<String> {
    let catalogue = catalogue(ir, component).expect("the component exists");
    assert_eq!(catalogue.component, component);
    let names = catalogue
        .counters
        .iter()
        .map(|counter| counter.name.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names.len(),
        catalogue.counters.len(),
        "no name is catalogued twice in {component}"
    );
    names
}

#[test]
fn every_command_outcome_and_event_of_a_component_has_a_counter() {
    for path in [rally(), relay()] {
        let loaded = spindle_spec::load(&path).expect("the specification compiles");
        let ir = &loaded.ir;
        assert!(!ir.components().is_empty());
        for component in ir.components().values() {
            let name = component.name.as_str();
            let catalogued = names(ir, name);
            let mut expected = BTreeSet::new();
            for accepted in &component.accepts {
                let command = ir.command(accepted);
                assert!(!command.outcomes.is_empty());
                for outcome in &command.outcomes {
                    expected.insert(format!("command.{accepted}.{}", outcome.name));
                    for event in &outcome.emits {
                        expected.insert(format!("event.{event}.appended"));
                    }
                }
                for failure in Failure::ALL {
                    if failure != Failure::UnknownCommand {
                        expected.insert(format!("command.{accepted}.failed.{}", failure.as_str()));
                    }
                }
                expected.insert(format!("command.{accepted}.duration_us"));
            }
            for event in &component.publishes {
                expected.insert(format!("event.{event}.appended"));
            }
            let missing = expected.difference(&catalogued).collect::<Vec<_>>();
            assert!(
                missing.is_empty(),
                "{name} has no counter for {missing:?}; it has {catalogued:?}"
            );
        }
    }
}

#[test]
fn every_binding_and_timer_of_a_component_has_its_counters() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let table = names(&loaded.ir, "rally-table");
    for name in [
        "timer.tick.fired",
        "timer.tick.late",
        "timer.tick.dropped",
        "binding.west-watches.sent",
        "binding.west-watches.redelivered",
        "binding.west-watches.dropped",
        "binding.east-watches.sent",
        "binding.west-moves.delivered",
        "binding.west-moves.dropped",
        "binding.east-moves.delivered",
        "store.rows_written",
        "store.bytes_written",
    ] {
        assert!(table.contains(name), "rally-table lacks {name}: {table:?}");
    }
    assert!(
        !table.contains("binding.west-watches.delivered"),
        "the table sends west-watches and never receives it"
    );
    let west = names(&loaded.ir, "rally-player-west");
    for name in [
        "binding.west-watches.delivered",
        "binding.west-moves.sent",
        "binding.west-moves.redelivered",
    ] {
        assert!(west.contains(name), "rally-player-west lacks {name}");
    }
    assert!(
        west.iter().all(|name| !name.starts_with("timer.")),
        "the west player accepts no periodic command: {west:?}"
    );
    assert!(west.iter().all(|name| !name.contains("east-")));

    let catalogue = catalogue(&loaded.ir, "rally-table").expect("rally-table exists");
    for counter in &catalogue.counters {
        let expected = if counter.name.starts_with("command.") {
            Source::Kernel
        } else if counter.name.starts_with("event.") || counter.name.starts_with("store.") {
            Source::Store
        } else if counter.name.starts_with("binding.") {
            Source::Channel
        } else {
            Source::Timer
        };
        assert_eq!(counter.source, expected, "{}", counter.name);
        let histogram = counter.name.ends_with(".duration_us");
        assert_eq!(
            counter.kind,
            if histogram {
                Kind::Histogram
            } else {
                Kind::Counter
            },
            "{}",
            counter.name
        );
    }
}

#[test]
fn the_catalogue_serialises_as_name_kind_and_source() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    let catalogue = catalogue(&loaded.ir, "rally-table").expect("rally-table exists");
    let json = serde_json::to_value(&catalogue).expect("the catalogue serialises");
    assert_eq!(json["component"], "rally-table");
    let first = &json["counters"][0];
    assert_eq!(
        first.as_object().map(|object| object.len()),
        Some(3),
        "{first}"
    );
    assert!(first["name"].is_string());
    assert!(first["kind"] == "counter" || first["kind"] == "histogram");
    assert_eq!(first["source"], "kernel");
}

#[test]
fn an_unknown_component_has_no_catalogue() {
    let loaded = spindle_spec::load(&rally()).expect("rally compiles");
    assert!(catalogue(&loaded.ir, "nowhere").is_none());
}

#[test]
fn every_kernel_failure_is_named_as_the_kernel_names_it() {
    let every = [
        ExecuteError::UnknownCommand("c".to_owned()),
        ExecuteError::Unsupported {
            construct: "c".to_owned(),
        },
        ExecuteError::InvalidInput("c".to_owned()),
        ExecuteError::Undecidable {
            outcome: "o".to_owned(),
        },
        ExecuteError::Undecided {
            capability: "c".to_owned(),
            reason: "r".to_owned(),
        },
        ExecuteError::NoValue {
            what: "w".to_owned(),
        },
        ExecuteError::InvalidValue("v".to_owned()),
        ExecuteError::BrokenInvariant {
            instance: "i".to_owned(),
            invariant: "v".to_owned(),
        },
        ExecuteError::Undeclared {
            command: "c".to_owned(),
            reason: "r".to_owned(),
        },
        ExecuteError::Store(StoreError::Backend("b".to_owned())),
    ];
    let mapped = every
        .iter()
        .map(|error| {
            let failure = Failure::from(error);
            assert_eq!(failure.as_str(), error.kind());
            failure
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        mapped,
        Failure::ALL.into_iter().collect(),
        "every failure is one kernel error"
    );
}
