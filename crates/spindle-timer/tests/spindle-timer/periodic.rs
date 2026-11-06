use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use spindle_timer::{
    BindingName, Clock, CompleteError, LifetimeError, ManualClock, Phase, Tick, Timer,
};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn rally() -> PathBuf {
    manifest().join("../../examples/rally/ess")
}

fn relay() -> PathBuf {
    manifest().join("../spindle-capability/tests/fixtures/relay")
}

fn activate(path: &Path, clock: &ManualClock) -> Timer {
    let loaded = spindle_spec::load(path).expect("the specification compiles");
    Timer::activate(&loaded.ir, clock)
}

fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn millis(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn named(ticks: &[Tick]) -> Vec<(String, String)> {
    ticks
        .iter()
        .map(|tick| (tick.binding.to_string(), tick.command.name().to_string()))
        .collect()
}

fn one(binding: &str, command: &str) -> Vec<(String, String)> {
    vec![(binding.to_owned(), command.to_owned())]
}

#[test]
fn a_periodic_binding_is_due_every_period() {
    for (path, binding, command) in [
        (rally(), "tick", "rally.table.Tick"),
        (relay(), "pulse-every-second", "relay.source.Pulse"),
    ] {
        let clock = ManualClock::new();
        let activation = clock.now();
        let mut timer = activate(&path, &clock);
        assert_eq!(
            timer
                .bindings()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
            vec![binding.to_owned()],
            "only the periodic binding is scheduled; event bindings are not the timer's"
        );

        for period in 1..=3 {
            assert_eq!(
                timer.next_deadline(),
                Some(activation + seconds(period)),
                "deadline {period} lies on the grid anchored at activation"
            );
            clock.advance(seconds(1));
            let due = timer.due(clock.now());
            assert_eq!(named(&due), one(binding, command), "period {period}");
            assert_eq!(due[0].deadline, activation + seconds(period));
            assert!(
                timer.due(clock.now()).is_empty(),
                "a period yields its tick once"
            );
            timer.complete(&due[0]).expect("the tick is open");
        }
        assert_eq!(timer.dropped(&ess_name(binding)), Some(0));
    }
}

#[test]
fn first_after_period_is_not_due_at_activation() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&rally(), &clock);

    assert!(timer.due(activation).is_empty(), "no tick at activation");
    assert_eq!(timer.next_deadline(), Some(activation + seconds(1)));

    clock.advance(millis(999));
    assert!(
        timer.due(clock.now()).is_empty(),
        "no tick before a full period"
    );

    clock.advance(millis(1));
    assert_eq!(
        named(&timer.due(clock.now())),
        one("tick", "rally.table.Tick")
    );
}

#[test]
fn overlap_serial_per_instance_never_yields_a_second_tick_while_one_is_open() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&rally(), &clock);

    clock.advance(seconds(1));
    let first = timer.due(clock.now());
    assert_eq!(named(&first), one("tick", "rally.table.Tick"));

    for _ in 0..3 {
        clock.advance(seconds(1));
        assert!(
            timer.due(clock.now()).is_empty(),
            "the first tick is still open"
        );
    }
    assert_eq!(
        timer.next_deadline(),
        None,
        "nothing can become due while the only binding has an open tick"
    );

    timer.complete(&first[0]).expect("the tick is open");
    let next = timer.due(clock.now());
    assert_eq!(named(&next), one("tick", "rally.table.Tick"));
    assert_eq!(
        next[0].deadline,
        activation + seconds(2),
        "fixed rate: the pending tick keeps its place on the grid"
    );

    let other = ManualClock::new();
    let mut relay_timer = activate(&relay(), &other);
    other.advance(seconds(1));
    let foreign = relay_timer.due(other.now());
    assert_eq!(
        timer.complete(&foreign[0]),
        Err(CompleteError::NotPeriodic(ess_name("pulse-every-second")))
    );
    timer.complete(&next[0]).expect("the tick is open");
    assert!(matches!(
        timer.complete(&next[0]),
        Err(CompleteError::Stale { .. })
    ));
}

#[test]
fn missed_ticks_keep_one_pending_and_drop_the_rest() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&relay(), &clock);
    let binding = ess_name("pulse-every-second");

    clock.advance(millis(3500));
    let late = timer.due(clock.now());
    assert_eq!(
        named(&late),
        one("pulse-every-second", "relay.source.Pulse"),
        "three deadlines passed unobserved: one tick"
    );
    assert_eq!(late[0].deadline, activation + seconds(1));
    assert_eq!(timer.dropped(&binding), Some(2));
    timer.complete(&late[0]).expect("the tick is open");
    assert!(
        timer.due(clock.now()).is_empty(),
        "the dropped deadlines are gone, not queued"
    );
    assert_eq!(timer.next_deadline(), Some(activation + seconds(4)));

    clock.advance(millis(500));
    let open = timer.due(clock.now());
    assert_eq!(open[0].deadline, activation + seconds(4));

    clock.advance(seconds(4));
    assert!(timer.due(clock.now()).is_empty(), "the tick is still open");
    timer.complete(&open[0]).expect("the tick is open");

    let pending = timer.due(clock.now());
    assert_eq!(
        named(&pending),
        one("pulse-every-second", "relay.source.Pulse")
    );
    assert_eq!(
        pending[0].deadline,
        activation + seconds(5),
        "the first missed deadline is the one kept"
    );
    assert_eq!(
        timer.dropped(&binding),
        Some(2 + 3),
        "deadlines 6, 7 and 8 passed while one was already pending"
    );
    timer.complete(&pending[0]).expect("the tick is open");
    assert!(timer.due(clock.now()).is_empty(), "exactly one was pending");
}

#[test]
fn stop_is_acknowledged_only_once_the_open_tick_completes() {
    let clock = ManualClock::new();
    let mut timer = activate(&rally(), &clock);

    clock.advance(seconds(1));
    let open = timer.due(clock.now());
    assert_eq!(open.len(), 1);
    clock.advance(seconds(1));

    timer.stop();
    assert!(!timer.quiescent(), "a tick is still open");
    clock.advance(seconds(5));
    assert!(
        timer.due(clock.now()).is_empty(),
        "a stopped timer yields nothing"
    );
    assert_eq!(timer.next_deadline(), None);

    timer.complete(&open[0]).expect("the tick is open");
    assert!(timer.quiescent());
    assert!(
        timer.due(clock.now()).is_empty(),
        "the pending tick died with the lifetime"
    );
}

fn pair() -> PathBuf {
    manifest().join("tests/fixtures/pair")
}

fn deadlines(ticks: &[Tick], activation: Instant) -> Vec<(String, Duration)> {
    ticks
        .iter()
        .map(|tick| (tick.binding.to_string(), tick.deadline - activation))
        .collect()
}

#[test]
fn each_binding_keeps_the_grid_of_its_own_activation() {
    let loaded = spindle_spec::load(&pair()).expect("the specification compiles");
    let mut timer = Timer::new(&loaded.ir);
    let fast = ess_name("fast");
    let slow = ess_name("slow");
    let t0 = ManualClock::new().now();

    assert_eq!(timer.phase(&fast), Some(Phase::Inactive));
    assert_eq!(timer.next_deadline(), None, "nothing is activated");
    assert!(timer.due(t0 + seconds(10)).is_empty());
    assert!(timer.quiescent());

    timer.activate_binding(&fast, t0).expect("fast activates");
    timer
        .activate_binding(&slow, t0 + millis(500))
        .expect("slow activates");
    assert_eq!(timer.phase(&slow), Some(Phase::Active));
    assert_eq!(
        timer.activate_binding(&slow, t0 + seconds(1)),
        Err(LifetimeError::Running(slow.clone()))
    );
    assert_eq!(
        timer.activate_binding(&ess_name("hear-pulse"), t0),
        Err(LifetimeError::NotPeriodic(ess_name("hear-pulse")))
    );

    let at2 = timer.due(t0 + seconds(2));
    assert_eq!(deadlines(&at2, t0), vec![("fast".to_owned(), seconds(2))]);
    assert!(
        timer.due(t0 + seconds(3)).is_empty(),
        "slow's grid starts at its own activation, not at fast's"
    );
    assert_eq!(timer.next_deadline(), Some(t0 + millis(3500)));
    let at3 = timer.due(t0 + millis(3500));
    assert_eq!(deadlines(&at3, t0), vec![("slow".to_owned(), millis(3500))]);

    timer.complete(&at2[0]).expect("fast is open");
    timer.complete(&at3[0]).expect("slow is open");
    timer.stop_binding(&fast).expect("fast is running");
    assert_eq!(
        timer.phase(&fast),
        Some(Phase::Inactive),
        "no open tick: the stop is acknowledged at once"
    );
    assert_eq!(
        timer.stop_binding(&fast),
        Err(LifetimeError::NotRunning(fast.clone()))
    );
    assert_eq!(
        deadlines(&timer.due(t0 + millis(6500)), t0),
        vec![("slow".to_owned(), millis(6500))],
        "stopping fast leaves slow's lifetime running"
    );

    timer
        .activate_binding(&fast, t0 + millis(7000))
        .expect("a new host instance");
    assert_eq!(
        timer.next_deadline(),
        Some(t0 + seconds(9)),
        "the new lifetime anchors a new grid"
    );
}

#[test]
fn a_stale_completion_is_refused_and_leaves_the_next_tick_open() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&relay(), &clock);
    let binding = ess_name("pulse-every-second");

    let first = timer.due(activation + seconds(1));
    timer.complete(&first[0]).expect("the first tick is open");
    let second = timer.due(activation + seconds(2));
    assert_eq!(second.len(), 1);

    assert_eq!(
        timer.complete(&first[0]),
        Err(CompleteError::Stale {
            binding: binding.clone(),
            deadline: activation + seconds(1),
        }),
        "a duplicate completion of the first tick"
    );
    assert!(
        timer.due(activation + seconds(3)).is_empty(),
        "the second tick is still open"
    );
    timer.complete(&second[0]).expect("the second tick is open");
    assert!(matches!(
        timer.complete(&second[0]),
        Err(CompleteError::Stale { .. })
    ));

    timer.stop_binding(&binding).expect("running");
    timer
        .activate_binding(&binding, activation)
        .expect("a new host instance");
    let again = timer.due(activation + seconds(1));
    assert_eq!(again[0].deadline, first[0].deadline);
    assert!(
        matches!(timer.complete(&first[0]), Err(CompleteError::Stale { .. })),
        "a tick from an earlier lifetime does not close one with the same deadline"
    );
    timer.complete(&again[0]).expect("the new tick is open");
}

fn ess_name(binding: &str) -> BindingName {
    BindingName::new(binding).expect("a valid binding name")
}
