//! The `rally` example played in one process, on a manual clock.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, HostError, ManualClock, Report};

const MATCH_STATE: &str = "rally.table.MatchState";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn record(report: &Report, ran: &mut BTreeSet<(String, String, &'static str)>) {
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.refused.is_empty(), "{:?}", report.refused);
    for step in &report.ran {
        let cause = match step.cause {
            Cause::Submitted => "submitted",
            Cause::Tick { .. } => "tick",
            Cause::Delivered { .. } => "delivered",
        };
        ran.insert((step.component.clone(), step.command.clone(), cause));
    }
}

#[test]
fn a_match_played_in_one_process_ends_with_a_winner() {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host =
        Host::new(loaded.ir, &laptop(), clock.clone(), state.path()).expect("laptop admits rally");

    let mut ran = BTreeSet::new();
    let opened = host
        .submit(
            "rally.table.OpenMatch",
            object(json!({"match_id": "m1", "target_score": 2})),
        )
        .expect("the match opens");
    record(&opened, &mut ran);
    assert_eq!(opened.ran[0].executed.outcome, "opened");

    assert_eq!(host.periodic(), vec!["tick".to_owned()]);
    host.start("tick", object(json!({"match_id": "m1"})))
        .expect("the tick binding starts");
    let still = host.advance().expect("the host advances");
    assert!(
        still.ran.is_empty(),
        "nothing is due before one period has passed: {:?}",
        still.ran
    );

    let mut winner = None;
    let mut ticks = 0;
    for _ in 0..4000 {
        clock.advance(Duration::from_secs(1));
        let advanced = host.advance().expect("the host advances");
        ticks += advanced
            .ran
            .iter()
            .filter(|step| matches!(step.cause, Cause::Tick { .. }))
            .count();
        assert!(
            advanced.ran.len() + advanced.refused.len() + advanced.failed.len() <= 5,
            "one tick of rally runs Tick, two ObserveBall and at most two MovePaddle: {advanced:?}"
        );
        record(&advanced, &mut ran);
        let view = host
            .view(MATCH_STATE, &json!("m1"))
            .expect("the view answers")
            .expect("the match exists");
        if let Some(side) = view.get("winner") {
            winner = Some((side.clone(), view));
            break;
        }
    }
    let (side, view) = winner.unwrap_or_else(|| panic!("no winner after {ticks} ticks"));
    assert!(ticks > 0, "the timer fired");
    assert_eq!(view["state"], json!("Won"));
    let (won, lost) = match side.as_str() {
        Some("West") => ("west_needs", "east_needs"),
        Some("East") => ("east_needs", "west_needs"),
        other => panic!("the winner is a side, not {other:?}"),
    };
    assert_eq!(view[won], json!(0));
    assert!(
        view[lost].as_i64().is_some_and(|needs| needs > 0),
        "{view:?}"
    );

    for (component, command, cause) in [
        ("rally-table", "rally.table.OpenMatch", "submitted"),
        ("rally-table", "rally.table.Tick", "tick"),
        ("rally-player-west", "rally.west.ObserveBall", "delivered"),
        ("rally-player-east", "rally.east.ObserveBall", "delivered"),
        ("rally-table", "rally.table.MovePaddle", "delivered"),
    ] {
        assert!(
            ran.contains(&(component.to_owned(), command.to_owned(), cause)),
            "{component} ran {command} ({cause}): {ran:?}"
        );
    }

    clock.advance(Duration::from_secs(1));
    let after = host.advance().expect("the host advances");
    assert_eq!(
        after
            .ran
            .iter()
            .map(|step| step.executed.outcome.as_str())
            .collect::<Vec<_>>(),
        ["match-over"],
        "a won match changes no more"
    );
    assert_eq!(
        host.view(MATCH_STATE, &json!("m1"))
            .expect("the view answers")
            .expect("the match exists"),
        view
    );
}

fn rally_host(state: &Path, clock: Arc<ManualClock>) -> Host {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    Host::new(loaded.ir, &laptop(), clock, state).expect("laptop admits rally")
}

#[test]
fn a_command_a_tick_invokes_and_the_model_refuses_is_reported_as_refused() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host = rally_host(state.path(), clock.clone());
    host.start("tick", object(json!({"match_id": "nobody-opened-this"})))
        .expect("the tick binding starts");
    clock.advance(Duration::from_secs(1));
    let advanced = host.advance().expect("the host advances");
    assert!(advanced.ran.is_empty(), "{:?}", advanced.ran);
    assert!(advanced.failed.is_empty(), "{:?}", advanced.failed);
    let refused = advanced
        .refused
        .iter()
        .map(|refused| {
            (
                refused.component.as_str(),
                refused.command.as_str(),
                refused.cause.clone(),
                refused.outcome.as_str(),
                refused.error.name.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        refused,
        [(
            "rally-table",
            "rally.table.Tick",
            Cause::Tick {
                binding: "tick".to_owned()
            },
            "no-such-match",
            "rally.table.MatchNotFound",
        )]
    );
}

#[test]
fn a_submitted_command_the_model_refuses_is_an_error_naming_the_declared_error() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = rally_host(state.path(), Arc::new(ManualClock::new()));
    match host.submit(
        "rally.table.OpenMatch",
        object(json!({"match_id": "m0", "target_score": 0})),
    ) {
        Err(HostError::CommandRefused { command, error, .. }) => {
            assert_eq!(command, "rally.table.OpenMatch");
            assert_eq!(error.name, "rally.table.TargetScoreTooLow");
        }
        other => panic!("a match to no points opened: {other:?}"),
    }
    assert_eq!(
        host.view(MATCH_STATE, &json!("m0"))
            .expect("the view answers"),
        None
    );
}

/// Runs the runaway case in a child process with a deadline, so a host without a bound fails the
/// case instead of growing its report until the machine swaps.
#[test]
fn bindings_that_trigger_each_other_stop_as_a_runaway_naming_them() {
    let mut child = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["runaway_child", "--exact", "--ignored", "--test-threads=1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the child starts");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("the child is observable") {
            let output = child.wait_with_output().expect("the child's output reads");
            assert!(
                status.success(),
                "the runaway case failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("one submit of an endless echo had not returned after 20 s: no delivery bound");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "spawned by bindings_that_trigger_each_other_stop_as_a_runaway_naming_them"]
fn runaway_child() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let echo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary/echo");
    let loaded = spindle_spec::load(&echo).expect("echo compiles");
    let mut host = Host::new(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("laptop admits echo");
    match host.submit("echo.left.Send", object(json!({"n": 1}))) {
        Err(HostError::Runaway {
            deliveries,
            bindings,
        }) => {
            assert_eq!(deliveries, spindle_host::DELIVERY_LIMIT);
            assert_eq!(
                bindings.into_iter().collect::<Vec<_>>(),
                ["left-to-right", "right-to-left"]
            );
        }
        Err(other) => panic!("one submit of an endless echo was not a runaway: {other}"),
        Ok(report) => panic!(
            "one submit of an endless echo returned a report of {} commands",
            report.ran.len()
        ),
    }
    let again = host
        .submit("echo.left.Send", object(json!({"n": 2})))
        .map(|report| report.ran.len());
    assert!(
        matches!(again, Err(HostError::Runaway { .. })),
        "the next submit starts from no backlog and runs away again on its own: {again:?}"
    );
}
