use std::path::{Path, PathBuf};
use std::time::Duration;

use spindle_timer::{BindingName, Clock, ManualClock, Tick, Timer};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn relay() -> PathBuf {
    manifest().join("../spindle-capability/tests/fixtures/relay")
}

fn pair() -> PathBuf {
    manifest().join("tests/fixtures/pair")
}

fn activate(path: &Path, clock: &ManualClock) -> Timer {
    let loaded = spindle_spec::load(path).expect("the specification compiles");
    Timer::activate(&loaded.ir, clock)
}

fn millis(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn name(binding: &str) -> BindingName {
    BindingName::new(binding).expect("a valid binding name")
}

fn bindings(ticks: &[Tick]) -> Vec<String> {
    ticks.iter().map(|tick| tick.binding.to_string()).collect()
}

#[test]
fn due_never_yields_a_tick_whose_deadline_is_after_the_now_it_was_asked_about() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&relay(), &clock);

    let first = timer.due(activation + seconds(1));
    assert_eq!(first.len(), 1);
    assert!(timer.due(activation + seconds(2)).is_empty(), "still open");
    timer.complete(&first[0]).expect("the tick is open");

    let now = activation + millis(1500);
    let ticks = timer.due(now);
    for tick in &ticks {
        assert!(
            tick.deadline <= now,
            "due({:?} after activation) yielded a tick for deadline {:?} after activation; \
             it is not due at that instant",
            now - activation,
            tick.deadline - activation
        );
    }
}

#[test]
fn two_periodic_bindings_keep_their_own_grids_and_their_own_overlap() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&pair(), &clock);
    assert_eq!(
        timer.bindings().map(|b| b.to_string()).collect::<Vec<_>>(),
        vec!["fast".to_owned(), "slow".to_owned()]
    );
    assert_eq!(timer.next_deadline(), Some(activation + seconds(2)));

    let at2 = timer.due(activation + seconds(2));
    assert_eq!(bindings(&at2), vec!["fast"]);
    assert_eq!(at2[0].command.name().to_string(), "relay.source.Pulse");
    assert_eq!(
        timer.next_deadline(),
        Some(activation + seconds(3)),
        "fast is open; slow's first deadline is still ahead"
    );

    let at3 = timer.due(activation + seconds(3));
    assert_eq!(
        bindings(&at3),
        vec!["slow"],
        "fast's open tick does not hold slow"
    );
    assert_eq!(at3[0].command.name().to_string(), "relay.sink.Hear");
    assert_eq!(timer.next_deadline(), None, "both bindings are open");

    timer.complete(&at3[0]).expect("slow is open");
    assert_eq!(timer.next_deadline(), Some(activation + seconds(6)));

    assert!(
        timer.due(activation + seconds(4)).is_empty(),
        "fast still open"
    );
    timer.complete(&at2[0]).expect("fast is open");
    assert_eq!(
        timer.next_deadline(),
        Some(activation + seconds(4)),
        "fast's deadline 4 is retained and due now"
    );
    let retained = timer.due(activation + millis(4500));
    assert_eq!(bindings(&retained), vec!["fast"]);
    assert_eq!(retained[0].deadline, activation + seconds(4));
    timer.complete(&retained[0]).expect("fast is open");

    let at6 = timer.due(activation + seconds(6));
    assert_eq!(bindings(&at6), vec!["fast", "slow"]);
    assert_eq!(at6[0].deadline, activation + seconds(6));
    assert_eq!(at6[1].deadline, activation + seconds(6));
    assert_eq!(timer.dropped(&name("fast")), Some(0));
    assert_eq!(timer.dropped(&name("slow")), Some(0));
}

#[test]
fn work_blocked_past_two_deadlines_releases_one_pending_then_rejoins_the_grid() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&pair(), &clock);
    let fast = name("fast");

    let started = timer.due(activation + seconds(2));
    assert_eq!(bindings(&started), vec!["fast"]);
    assert_eq!(started[0].deadline, activation + seconds(2));

    let released = activation + millis(6500);
    timer.complete(&started[0]).expect("fast is open");
    let ticks: Vec<Tick> = timer
        .due(released)
        .into_iter()
        .filter(|tick| tick.binding == fast)
        .collect();
    assert_eq!(ticks.len(), 1, "at most one pending dispatch on release");
    assert_eq!(
        ticks[0].deadline,
        activation + seconds(4),
        "the retained tick keeps its old ordinal"
    );
    assert_eq!(timer.dropped(&fast), Some(1), "deadline 6 was dropped");
    timer.complete(&ticks[0]).expect("fast is open");
    assert!(
        timer.due(released).iter().all(|tick| tick.binding != fast),
        "no backlog"
    );

    let rejoined: Vec<Tick> = timer
        .due(activation + seconds(8))
        .into_iter()
        .filter(|tick| tick.binding == fast)
        .collect();
    assert_eq!(rejoined.len(), 1);
    assert_eq!(
        rejoined[0].deadline,
        activation + seconds(8),
        "the original grid"
    );
}

#[test]
fn next_deadline_is_the_retained_tick_once_its_binding_is_idle() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&relay(), &clock);

    let first = timer.due(activation + seconds(1));
    assert_eq!(first.len(), 1);
    assert!(timer.due(activation + millis(2500)).is_empty(), "open");
    timer.complete(&first[0]).expect("the tick is open");

    assert_eq!(
        timer.next_deadline(),
        Some(activation + seconds(2)),
        "the retained tick is due now, not at the next grid deadline 3"
    );
}

#[test]
fn completing_late_every_period_never_drifts_the_fixed_rate_grid() {
    let clock = ManualClock::new();
    let activation = clock.now();
    let mut timer = activate(&relay(), &clock);
    let binding = name("pulse-every-second");

    for n in 1..=1000u64 {
        let deadline = activation + seconds(n);
        assert_eq!(timer.next_deadline(), Some(deadline), "ordinal {n}");
        let ticks = timer.due(deadline);
        assert_eq!(ticks.len(), 1, "ordinal {n}");
        assert_eq!(ticks[0].deadline, deadline, "ordinal {n}");
        assert!(timer.due(deadline + millis(999)).is_empty());
        timer.complete(&ticks[0]).expect("the tick is open");
    }
    assert_eq!(timer.dropped(&binding), Some(0));
}
