//! Adversary cases for `run --scenario` (story:replay-ess-scenarios), over the neutral desk
//! fixture and variants of it written into a temporary directory.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/replay";
const LAPTOP: &str = "examples/environments/laptop.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

fn replay(spec: &Path, environment: &Path, scenario: &Path) -> Output {
    spindle(&[
        "run",
        spec.to_str().expect("utf-8 path"),
        "--environment",
        environment.to_str().expect("utf-8 path"),
        "--scenario",
        scenario.to_str().expect("utf-8 path"),
    ])
}

fn write(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("directory");
    }
    std::fs::write(&path, text).expect("written");
    path
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The desk fixture copied into `directory`, with `views` appended to its views.
fn desk_with_views(directory: &Path, views: &str) -> PathBuf {
    let source = root().join(SPEC);
    let spec = directory.join("spec");
    let system = std::fs::read_to_string(source.join("system.yaml")).expect("system");
    let domain =
        std::fs::read_to_string(source.join("domains/ticket.yaml")).expect("ticket domain");
    write(&spec, "system.yaml", &system);
    write(&spec, "domains/ticket.yaml", &format!("{domain}{views}"));
    spec
}

fn laptop() -> PathBuf {
    root().join(LAPTOP)
}

fn moment(second: usize) -> String {
    format!(
        "2026-01-05T{:02}:{:02}:{:02}Z",
        second / 3600,
        (second / 60) % 60,
        second % 60
    )
}

/// A scenario of `acts` openings, each holding.
fn openings(acts: usize) -> String {
    let mut text = String::from(
        "type: ess-scenario/1\ndomain: desk.ticket\nscenario: many-openings\n\
         summary: Many tickets open one after another.\ntimeline:\n",
    );
    for act in 0..acts {
        let _ = write!(
            text,
            "  - at: {}\n    command: desk.ticket.OpenTicket\n    input:\n      title: t{act}\n    outcome: opened\n",
            moment(act)
        );
    }
    text
}

fn timed(spec: &Path, scenario: &Path) -> (Output, Duration) {
    let started = Instant::now();
    let output = replay(spec, &laptop(), scenario);
    (output, started.elapsed())
}

/// The replay re-runs every prefix from an empty system, so N document steps cost N host starts
/// and N(N+1)/2 command executions. Four times the steps must not cost much more than four times
/// the time.
#[test]
fn a_long_scenario_replays_in_time_linear_in_its_steps() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let spec = root().join(SPEC);
    let short = write(directory.path(), "short.yaml", &openings(60));
    let long = write(directory.path(), "long.yaml", &openings(240));
    let (output, short_took) = timed(&spec, &short);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let (output, long_took) = timed(&spec, &long);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        long_took < short_took * 8,
        "60 acts took {short_took:?}, 240 acts took {long_took:?}: {:.1}x for 4x the steps",
        long_took.as_secs_f64() / short_took.as_secs_f64()
    );
}

/// A scenario has no step bound: a document of a few thousand acts runs every prefix.
#[test]
fn a_scenario_past_any_reasonable_length_is_bounded() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let spec = root().join(SPEC);
    let scenario = write(directory.path(), "huge.yaml", &openings(1200));
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            spec.to_str().expect("utf-8 path"),
            "--environment",
            laptop().to_str().expect("utf-8 path"),
            "--scenario",
            scenario.to_str().expect("utf-8 path"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("b10x-spindle starts");
    let started = Instant::now();
    let bound = Duration::from_secs(30);
    let mut finished = None;
    for _ in 0..300 {
        if let Some(status) = child.try_wait().expect("polled") {
            finished = Some(status);
            break;
        }
        if started.elapsed() > bound {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if finished.is_none() {
        child.kill().expect("killed");
        child.wait().expect("reaped");
    }
    assert!(
        finished.is_some(),
        "1200 acts still running after {:?}; no bound refused or finished the document",
        started.elapsed()
    );
}

#[test]
fn a_refused_admission_exits_three_before_any_step() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let laptop = std::fs::read_to_string(laptop()).expect("laptop");
    let without_mint = laptop
        .lines()
        .filter(|line| !line.starts_with("\"identity.mint\""))
        .collect::<Vec<_>>()
        .join("\n");
    let environment = write(directory.path(), "no-mint.toml", &without_mint);
    let scenario = root().join("crates/spindle-cli/tests/fixtures/replay-scenarios/holds.yaml");
    let output = replay(&root().join(SPEC), &environment, &scenario);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(!stdout(&output).contains("timeline["), "{output:?}");
}

#[test]
fn scenario_refuses_each_flag_of_the_stdin_run() {
    let scenario = "crates/spindle-cli/tests/fixtures/replay-scenarios/holds.yaml";
    for extra in [
        &["--state", "x.sqlite"][..],
        &["--decisions", "x.toml"][..],
        &["--answer-default"][..],
    ] {
        let mut arguments = vec!["run", SPEC, "--environment", LAPTOP, "--scenario", scenario];
        arguments.extend_from_slice(extra);
        let output = spindle(&arguments);
        assert_eq!(output.status.code(), Some(2), "{extra:?}: {output:?}");
        assert!(
            stderr(&output).contains("cannot be used with"),
            "{output:?}"
        );
    }
}

/// An arrangement the in-process host cannot make, a fixture value it does not provide, is the
/// arrangement's failure; naming the first timeline act as the step that diverged points the
/// author at a command that never ran.
#[test]
fn an_arrangement_that_cannot_be_established_is_not_named_as_the_first_act() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let source = root().join(SPEC);
    let spec = directory.path().join("spec");
    let domain =
        std::fs::read_to_string(source.join("domains/ticket.yaml")).expect("ticket domain");
    let fixtured = domain.replacen(
        "    outcomes:\n      - name: opened\n",
        "    fixture_inputs:\n      title: current-title\n    outcomes:\n      - name: opened\n",
        1,
    );
    assert_ne!(
        fixtured, domain,
        "OpenTicket takes its title from a fixture"
    );
    write(
        &spec,
        "system.yaml",
        &std::fs::read_to_string(source.join("system.yaml")).expect("system"),
    );
    write(&spec, "domains/ticket.yaml", &fixtured);
    let scenario = write(
        directory.path(),
        "fixture.yaml",
        "type: ess-scenario/3\n\
         domain: desk.ticket\n\
         scenario: a-ticket-titled-from-a-fixture-opens\n\
         summary: A ticket titled from a fixture value opens.\n\
         fixtures:\n\
         \x20 current-title: String\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input:\n\
         \x20     title: {$fixture: current-title}\n\
         \x20   outcome: opened\n",
    );
    let output = replay(&spec, &laptop(), &scenario);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        printed.contains("arrange: diverges"),
        "the fixture was never provided: {output:?}"
    );
    assert!(
        !printed.contains("timeline[0] desk.ticket.OpenTicket: diverges"),
        "the open never ran; the arrangement did not hold: {printed}"
    );
}

/// An entity the document establishes is written into the store of the component that owns it,
/// and the act after it reads it: every step holds.
#[test]
fn an_established_arrangement_is_read_by_the_first_act() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let scenario = write(
        directory.path(),
        "setup.yaml",
        "type: ess-scenario/2\n\
         domain: desk.ticket\n\
         scenario: an-established-ticket-closes\n\
         summary: A ticket established as open closes.\n\
         arrange:\n\
         \x20 - instance: printer\n\
         \x20   entity: desk.ticket.Ticket\n\
         \x20   setup:\n\
         \x20     identity: 00000000-0000-4000-8000-000000000001\n\
         \x20     fields: {title: printer}\n\
         \x20     state: Open\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: desk.ticket.CloseTicket\n\
         \x20   input:\n\
         \x20     ticket_id: {$instance: printer}\n\
         \x20   outcome: closed\n\
         assert:\n\
         \x20 - view: desk.ticket.AllTickets\n\
         \x20   contains: {ticket_id: 00000000-0000-4000-8000-000000000001, state: Closed, title: printer}\n",
    );
    let output = replay(&root().join(SPEC), &laptop(), &scenario);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for step in [
        "arrange: holds",
        "timeline[0] desk.ticket.CloseTicket: holds",
        "assert[0] desk.ticket.AllTickets: holds",
    ] {
        assert!(printed.contains(step), "{step}: {printed}");
    }
}

#[test]
fn an_instance_captured_after_the_first_act_still_replays() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let scenario = write(
        directory.path(),
        "late-capture.yaml",
        "type: ess-scenario/1\n\
         domain: desk.ticket\n\
         scenario: a-late-capture\n\
         summary: The second ticket is captured and listed.\n\
         arrange:\n\
         \x20 - instance: printer\n\
         \x20   entity: desk.ticket.Ticket\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {title: kettle}\n\
         \x20   outcome: opened\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {title: printer}\n\
         \x20   outcome: opened\n\
         \x20   capture: {instance: printer, event: desk.ticket.TicketOpened, field: ticket_id}\n\
         assert:\n\
         \x20 - view: desk.ticket.OpenTickets\n\
         \x20   contains: {ticket_id: {$instance: printer}, title: printer}\n",
    );
    let output = replay(&root().join(SPEC), &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        stdout(&output).contains("assert[0] desk.ticket.OpenTickets: holds"),
        "{output:?}"
    );
}

/// One act, three claims: the outcome holds, the event's payload does not. The act is the
/// divergent step and the payload check is printed.
#[test]
fn a_wrong_payload_on_a_right_outcome_names_the_act() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let scenario = write(
        directory.path(),
        "payload.yaml",
        "type: ess-scenario/1\n\
         domain: desk.ticket\n\
         scenario: a-wrong-payload\n\
         summary: The opened event is claimed with the wrong title.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {title: kettle}\n\
         \x20   outcome: opened\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {title: printer}\n\
         \x20   outcome: opened\n\
         \x20   events:\n\
         \x20     - event: desk.ticket.TicketOpened\n\
         \x20       payload: {title: kettle}\n\
         \x20 - at: 2026-01-05T09:02:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {title: lamp}\n\
         \x20   outcome: opened\n",
    );
    let output = replay(&root().join(SPEC), &laptop(), &scenario);
    let printed = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        printed.contains("timeline[0] desk.ticket.OpenTicket: holds"),
        "{printed}"
    );
    assert!(
        printed.contains("timeline[1] desk.ticket.OpenTicket: diverges"),
        "{printed}"
    );
    assert!(!printed.contains("timeline[2]"), "{printed}");
    assert!(printed.contains("TicketOpened"), "{printed}");
}

const ORDERED_VIEWS: &str = "  - name: desk.ticket.ByTitle
    source: desk.ticket.Ticket
    consistency: read_your_writes
    order_by: [title asc]
    fields:
      - name: ticket_id
        type: desk.ticket.TicketId
      - name: title
        type: String
  - name: desk.ticket.EventuallyAll
    source: desk.ticket.Ticket
    consistency: eventual
    fields:
      - name: ticket_id
        type: desk.ticket.TicketId
      - name: title
        type: String
";

fn ordered_scenario(assertions: &str) -> String {
    format!(
        "type: ess-scenario/1\n\
         domain: desk.ticket\n\
         scenario: ordered-claims\n\
         summary: Claims over an ordered and an eventual view.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {{title: kettle}}\n\
         \x20   outcome: opened\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: desk.ticket.OpenTicket\n\
         \x20   input: {{title: apple}}\n\
         \x20   outcome: opened\n\
         assert:\n{assertions}"
    )
}

#[test]
fn ordered_counted_and_eventual_claims_are_read_from_the_host() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory");
    let spec = desk_with_views(directory.path(), ORDERED_VIEWS);
    let holding = write(
        directory.path(),
        "holding.yaml",
        &ordered_scenario(
            "  - view: desk.ticket.ByTitle\n    at: {row: first, fields: {title: apple}}\n\
             \x20 - view: desk.ticket.ByTitle\n    counts: {at_least: 2, at_most: 2}\n\
             \x20 - view: desk.ticket.EventuallyAll\n    contains: {title: kettle}\n",
        ),
    );
    let output = replay(&spec, &laptop(), &holding);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for (index, claim) in [
        "  - view: desk.ticket.ByTitle\n    at: {row: first, fields: {title: kettle}}\n",
        "  - view: desk.ticket.ByTitle\n    counts: {at_least: 3}\n",
        "  - view: desk.ticket.ByTitle\n    excludes: {title: apple}\n",
        "  - view: desk.ticket.EventuallyAll\n    contains: {title: lamp}\n",
    ]
    .into_iter()
    .enumerate()
    {
        let scenario = write(
            directory.path(),
            &format!("wrong-{index}.yaml"),
            &ordered_scenario(claim),
        );
        let started = Instant::now();
        let output = replay(&spec, &laptop(), &scenario);
        assert!(started.elapsed() < Duration::from_secs(30), "{claim}");
        assert_eq!(output.status.code(), Some(1), "{claim}: {output:?}");
        assert!(stdout(&output).contains("assert[0]"), "{claim}: {output:?}");
        assert!(
            stdout(&output).contains(": diverges"),
            "{claim}: {output:?}"
        );
    }
}
