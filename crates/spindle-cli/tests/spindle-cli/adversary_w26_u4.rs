//! Adversary cases of wave 26, unit U4 (`story:scenario-act-without-outcome`,
//! `story:scenario-without-components`): which command answer each act's line carries, and the
//! implied component of a specification that declares none.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, AdvancingClock, ConformanceSuite, Ids, Runner, RunnerConfig, Status,
    SuiteProvenance,
};

const LAPTOP: &str = "examples/environments/laptop.toml";
const REFUSALS: &str = "crates/spindle-cli/tests/fixtures/replay-refusals";
const SEED: &str = "crates/spindle-cli/tests/fixtures/seed";
const WITHOUT_COMPONENTS: &str = "crates/spindle-cli/tests/fixtures/replay-without-components.yaml";
const NO_DURABLE_STORE: &str = "crates/spindle-cli/tests/fixtures/no-durable-store.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str], lines: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(if lines.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    if let Some(lines) = lines {
        let written = child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(lines.as_bytes());
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
        }
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

fn replay(spec: &Path, environment: &Path, scenario: &Path) -> Output {
    spindle(
        &[
            "run",
            spec.to_str().expect("utf-8 path"),
            "--environment",
            environment.to_str().expect("utf-8 path"),
            "--scenario",
            scenario.to_str().expect("utf-8 path"),
        ],
        None,
    )
}

fn stdin_run(spec: &Path, environment: &Path, lines: &str) -> Output {
    spindle(
        &[
            "run",
            spec.to_str().expect("utf-8 path"),
            "--environment",
            environment.to_str().expect("utf-8 path"),
        ],
        Some(lines),
    )
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

fn laptop() -> PathBuf {
    root().join(LAPTOP)
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("directory")
}

/// The status ESS 0.99.1's own runner gives the scenario `document` against ESS's interpreter of
/// the specification at `spec`.
fn ess_runner_status(spec: &Path, document: &str) -> Status {
    let ir = spindle_spec::load(spec)
        .expect("the specification compiles")
        .ir;
    let authoring = authored::compile(
        &ir,
        &[Source::new("scenario.yaml".to_owned(), document.to_owned())],
    );
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(&ir));
    for (id, compiled) in authoring.scenarios {
        suite.insert(id, compiled).expect("one scenario");
    }
    suite.select_fresh_format_for(&ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let target = Interpreted::for_model(ir.clone());
    let report = Runner::new(
        RunnerConfig::default(),
        AdvancingClock::default(),
        Ids::for_suite(&suite),
    )
    .run_admitted(&admitted, &target)
    .into_report();
    report.scenarios[0].status
}

fn assert_lines(printed: &str, lines: &[&str]) {
    for line in lines {
        assert!(printed.contains(line), "{line:?} in {printed}");
    }
}

/// The door of `REFUSALS`, opened, shut, shut again without an outcome, then shut once more
/// stating an outcome it does not take, and opened again: the line of each act before the
/// divergence carries that act's own answer, the divergent line the answer of the act that
/// diverged, and nothing after it runs.
#[test]
fn w26_u4_each_line_keeps_its_own_answer_across_a_capture_refusals_and_a_divergence() {
    let directory = scratch();
    let document = "type: ess-scenario/1\n\
         domain: hatch.door\n\
         scenario: a-door-shut-three-times\n\
         summary: A door opens and is shut three times; the last act states the wrong outcome.\n\
         arrange:\n\
         \x20 - instance: front\n\
         \x20   entity: hatch.door.Door\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: hatch.door.OpenDoor\n\
         \x20   input:\n\
         \x20     label: front\n\
         \x20   outcome: opened\n\
         \x20   capture: {instance: front, event: hatch.door.DoorOpened, field: door_id}\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: hatch.door.ShutDoor\n\
         \x20   input:\n\
         \x20     door_id: {$instance: front}\n\
         \x20 - at: 2026-01-05T09:02:00Z\n\
         \x20   command: hatch.door.ShutDoor\n\
         \x20   input:\n\
         \x20     door_id: {$instance: front}\n\
         \x20 - at: 2026-01-05T09:03:00Z\n\
         \x20   command: hatch.door.ShutDoor\n\
         \x20   input:\n\
         \x20     door_id: {$instance: front}\n\
         \x20   outcome: shut\n\
         \x20 - at: 2026-01-05T09:04:00Z\n\
         \x20   command: hatch.door.OpenDoor\n\
         \x20   input:\n\
         \x20     label: back\n";
    let spec = root().join(REFUSALS);
    assert_eq!(ess_runner_status(&spec, document), Status::Failed);
    let scenario = write(directory.path(), "shut-thrice.yaml", document);
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert_lines(
        &printed,
        &[
            "timeline[0] hatch.door.OpenDoor: holds, outcome opened\n",
            "timeline[1] hatch.door.ShutDoor: holds, outcome shut\n",
            "timeline[2] hatch.door.ShutDoor: holds, outcome already-shut, error hatch.door.AlreadyShut\n",
            "timeline[3] hatch.door.ShutDoor: diverges, outcome already-shut, error hatch.door.AlreadyShut\n",
        ],
    );
    assert!(!printed.contains("timeline[4]"), "{printed}");
}

/// An act that expects `refused: not_granted` prints `not granted`, and the act after it, which
/// states no outcome and is refused with a declared error, prints its own answer, not the one
/// before it.
#[test]
fn w26_u4_a_not_granted_act_prints_it_and_the_next_act_its_own_answer() {
    let directory = scratch();
    let document = "type: ess-scenario/4\n\
         domain: vault.access\n\
         scenario: a-visitor-may-not-retire\n\
         summary: A visitor is refused a retirement; an operator retires a role nobody granted.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: vault.access.RetireRole\n\
         \x20   actor: vault.access.Visitor\n\
         \x20   input:\n\
         \x20     role_id: admin\n\
         \x20   refused: not_granted\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: vault.access.RetireRole\n\
         \x20   actor: vault.access.Operator\n\
         \x20   input:\n\
         \x20     role_id: admin\n";
    let spec = root().join(SEED);
    assert_eq!(ess_runner_status(&spec, document), Status::Passed);
    let scenario = write(directory.path(), "not-granted.yaml", document);
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_lines(
        &stdout(&output),
        &[
            "timeline[0] vault.access.RetireRole: holds, not granted\n",
            "timeline[1] vault.access.RetireRole: holds, outcome missing, error vault.access.RoleMissing\n",
        ],
    );
}

/// A specification without components refused at admission names what it lacks without naming a
/// component the specification does not declare: the consumer reads `implied` nowhere in their
/// documents, and stdin mode, which the story's acceptance compares against, names none.
#[test]
fn w26_u4_a_refused_specification_without_components_names_no_undeclared_component() {
    let directory = scratch();
    let scenario = write(
        directory.path(),
        "open.yaml",
        "type: ess-scenario/1\n\
         domain: hatch.door\n\
         scenario: a-door-opens\n\
         summary: A door opens.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: hatch.door.OpenDoor\n\
         \x20   input:\n\
         \x20     label: front\n",
    );
    let spec = root().join(WITHOUT_COMPONENTS);
    let environment = root().join(NO_DURABLE_STORE);
    let stdin = stdin_run(&spec, &environment, "");
    assert_eq!(stdin.status.code(), Some(3), "{stdin:?}");
    assert!(!stdout(&stdin).contains("implied"), "{stdin:?}");

    let output = replay(&spec, &environment, &scenario);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let printed = stdout(&output);
    assert!(printed.contains("store.durable"), "{printed}");
    assert!(
        !printed.contains("implied"),
        "the refusal names a component the specification does not declare: {printed}"
    );
}

/// The door specification without components, with `components: []` written out in its system
/// document: it declares no component, so it replays as one implied component.
#[test]
fn w26_u4_an_explicitly_empty_component_list_replays() {
    let directory = scratch();
    let source = std::fs::read_to_string(root().join(WITHOUT_COMPONENTS)).expect("spec");
    let text = source.replacen(
        "domains:\n  - hatch.door\n",
        "domains:\n  - hatch.door\ncomponents: []\n",
        1,
    );
    assert_ne!(text, source, "the component list is written");
    let spec = write(directory.path(), "hatch.yaml", &text);
    let stdin = stdin_run(
        &spec,
        &laptop(),
        "{\"command\": \"hatch.door.OpenDoor\", \"input\": {\"label\": \"front\"}}\n",
    );
    assert_eq!(stdin.status.code(), Some(0), "{stdin:?}");
    let scenario = write(
        directory.path(),
        "open.yaml",
        "type: ess-scenario/1\n\
         domain: hatch.door\n\
         scenario: a-door-opens\n\
         summary: A door opens.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: hatch.door.OpenDoor\n\
         \x20   input:\n\
         \x20     label: front\n",
    );
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_lines(
        &stdout(&output),
        &["timeline[0] hatch.door.OpenDoor: holds, outcome opened\n"],
    );
}

/// A specification without components that declares no event: the implied component publishes
/// none, and the replay still runs as stdin mode does.
#[test]
fn w26_u4_a_specification_without_components_or_events_replays() {
    let directory = scratch();
    let spec = write(
        directory.path(),
        "chime.yaml",
        "format: ess/34\n\
         system: chime\n\
         version: v1\n\
         summary: Notes refused, in a system that declares no component and no event.\n\
         domains:\n\
         \x20 - chime.note\n\
         domain: chime.note\n\
         errors:\n\
         \x20 - name: chime.note.Closed\n\
         \x20   summary: The book of notes is closed.\n\
         \x20   fields: []\n\
         commands:\n\
         \x20 - name: chime.note.KeepNote\n\
         \x20   input:\n\
         \x20     - name: note_id\n\
         \x20       type: String\n\
         \x20     - name: text\n\
         \x20       type: String\n\
         \x20   outcomes:\n\
         \x20     - name: closed\n\
         \x20       error: chime.note.Closed\n\
         \x20       summary: Every note is refused.\n",
    );
    let stdin = stdin_run(
        &spec,
        &laptop(),
        "{\"command\": \"chime.note.KeepNote\", \"input\": {\"note_id\": \"n1\", \"text\": \"hi\"}}\n",
    );
    assert_eq!(stdin.status.code(), Some(0), "{stdin:?}");
    let scenario = write(
        directory.path(),
        "keep.yaml",
        "type: ess-scenario/1\n\
         domain: chime.note\n\
         scenario: a-note-is-kept\n\
         summary: A note is kept.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: chime.note.KeepNote\n\
         \x20   input:\n\
         \x20     note_id: n1\n\
         \x20     text: hi\n",
    );
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_lines(
        &stdout(&output),
        &["timeline[0] chime.note.KeepNote: holds, outcome closed, error chime.note.Closed\n"],
    );
}

/// A specification whose one declared component is named `implied` and does not accept
/// `hatch.door.ShutDoor` keeps its own IR: the shutting is still accepted by no component.
#[test]
fn w26_u4_a_declared_component_named_implied_still_routes_by_accepts() {
    let directory = scratch();
    let source = std::fs::read_to_string(
        root().join("crates/spindle-cli/tests/fixtures/replay-unaccepted.yaml"),
    )
    .expect("spec");
    let text = source.replacen("component: opener", "component: implied", 1);
    assert_ne!(text, source, "the component is renamed");
    let spec = write(directory.path(), "hatch.yaml", &text);
    let scenario = write(
        directory.path(),
        "shut.yaml",
        "type: ess-scenario/1\n\
         domain: hatch.door\n\
         scenario: a-door-no-one-opened-is-shut\n\
         summary: A door no one opened is shut.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: hatch.door.ShutDoor\n\
         \x20   input:\n\
         \x20     door_id: 0b5e0000-0000-4000-8000-000000000001\n",
    );
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let printed = stdout(&output);
    assert_lines(
        &printed,
        &[
            "timeline[0] hatch.door.ShutDoor: diverges\n",
            "no component accepts hatch.door.ShutDoor",
        ],
    );
}

/// Two domains and no component: the implied component owns both, so an act of either domain's
/// command runs in one replay.
#[test]
fn w26_u4_two_domains_without_components_replay_in_one_host() {
    let directory = scratch();
    let spec = directory.path().join("spec");
    write(
        &spec,
        "system.yaml",
        "format: ess/34\n\
         system: hatch\n\
         version: v1\n\
         summary: A door and a bell, in a system that declares no component.\n\
         domains:\n\
         \x20 - hatch.door\n\
         \x20 - hatch.bell\n",
    );
    let door =
        std::fs::read_to_string(root().join(REFUSALS).join("domains/door.yaml")).expect("door");
    write(&spec, "domains/door.yaml", &door);
    write(
        &spec,
        "domains/bell.yaml",
        "domain: hatch.bell\n\
         summary: A bell hung once.\n\
         types:\n\
         \x20 - name: hatch.bell.BellId\n\
         \x20   kind: newtype\n\
         \x20   of: String\n\
         entities:\n\
         \x20 - name: hatch.bell.Bell\n\
         \x20   identity:\n\
         \x20     name: bell_id\n\
         \x20     type: hatch.bell.BellId\n\
         \x20   fields:\n\
         \x20     - name: tone\n\
         \x20       type: String\n\
         \x20   lifecycle:\n\
         \x20     initial: Hung\n\
         \x20     states: [Hung]\n\
         \x20     terminal: [Hung]\n\
         commands:\n\
         \x20 - name: hatch.bell.HangBell\n\
         \x20   input:\n\
         \x20     - name: bell_id\n\
         \x20       type: hatch.bell.BellId\n\
         \x20     - name: tone\n\
         \x20       type: String\n\
         \x20   outcomes:\n\
         \x20     - name: hung\n\
         \x20       creates: hatch.bell.Bell\n\
         \x20       instance: bell_id\n\
         \x20       sets:\n\
         \x20         tone: input.tone\n\
         \x20       emits: [hatch.bell.BellHung]\n\
         \x20       payload:\n\
         \x20         hatch.bell.BellHung: {bell_id: input.bell_id}\n\
         \x20       summary: The bell is hung.\n\
         events:\n\
         \x20 - name: hatch.bell.BellHung\n\
         \x20   fields:\n\
         \x20     - name: bell_id\n\
         \x20       type: hatch.bell.BellId\n",
    );
    let scenario = write(
        directory.path(),
        "both.yaml",
        "type: ess-scenario/1\n\
         domain: hatch.door\n\
         scenario: a-door-opens-and-a-bell-is-hung\n\
         summary: A door opens and a bell is hung.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: hatch.door.OpenDoor\n\
         \x20   input:\n\
         \x20     label: front\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: hatch.bell.HangBell\n\
         \x20   input:\n\
         \x20     bell_id: b1\n\
         \x20     tone: low\n",
    );
    let output = replay(&spec, &laptop(), &scenario);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_lines(
        &stdout(&output),
        &[
            "timeline[0] hatch.door.OpenDoor: holds, outcome opened\n",
            "timeline[1] hatch.bell.HangBell: holds, outcome hung\n",
        ],
    );
}

/// The story's untraced exception: an act that states no outcome, on a command whose every
/// outcome returns, still compiles to a direct-response claim, so an undeclared refusal of it
/// does not pass in ESS's runner (its interpreter answers `error`). The replay agrees with ESS's
/// verdict: it exits 1 and prints the refusal on the divergent line.
#[test]
fn w26_u4_an_outcome_less_act_on_a_returning_command_diverges_as_ess_fails_it() {
    let directory = scratch();
    let document = "type: ess-scenario/1\n\
         domain: vault.locker\n\
         scenario: a-locker-numbered-twice\n\
         summary: A locker is numbered twice by acts that state no outcome; the second is refused.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: vault.locker.NumberLocker\n\
         \x20   input:\n\
         \x20     locker_id: l1\n\
         \x20     label: first\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: vault.locker.NumberLocker\n\
         \x20   input:\n\
         \x20     locker_id: l1\n\
         \x20     label: again\n";
    let source = root().join("crates/spindle-cli/tests/fixtures/adversary-outcome-bodies");
    let spec = directory.path().join("spec");
    let system = std::fs::read_to_string(source.join("system.yaml")).expect("system");
    let in_process = system.replacen("reached_by: network", "reached_by: in_process", 1);
    assert_ne!(in_process, system, "the component is reached in process");
    write(&spec, "system.yaml", &in_process);
    write(
        &spec,
        "domains/locker.yaml",
        &std::fs::read_to_string(source.join("domains/locker.yaml")).expect("locker"),
    );
    let status = ess_runner_status(&spec, document);
    let scenario = write(directory.path(), "twice.yaml", document);
    let output = replay(&spec, &laptop(), &scenario);
    let printed = stdout(&output);
    assert_ne!(status, Status::Passed, "{printed}");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_lines(
        &printed,
        &[
            "timeline[0] vault.locker.NumberLocker: holds, outcome numbered\n",
            "timeline[1] vault.locker.NumberLocker: diverges, undeclared\n",
        ],
    );
}

/// The host's notice fixture, a binding from one domain's event to another domain's command,
/// replayed once with its two declared components and once with them removed: the implied
/// component dispatches the binding in process, so both replays answer alike.
#[test]
fn w26_u4_a_binding_runs_alike_with_declared_and_implied_components() {
    let directory = scratch();
    let source = root().join("crates/spindle-host/tests/fixtures/adversary/notice");
    let read = |name: &str| std::fs::read_to_string(source.join(name)).expect("notice fixture");
    let components = read("components.yaml");
    let (_, bindings) = components
        .split_once("bindings:\n")
        .expect("the fixture declares its binding beside its components");
    let declared = directory.path().join("declared");
    let implied = directory.path().join("implied");
    for spec in [&declared, &implied] {
        write(spec, "system.yaml", &read("system.yaml"));
        write(spec, "source.yaml", &read("source.yaml"));
        write(spec, "sink.yaml", &read("sink.yaml"));
    }
    write(&declared, "components.yaml", &components);
    write(&implied, "bindings.yaml", &format!("bindings:\n{bindings}"));
    let scenario = write(
        directory.path(),
        "pin.yaml",
        "type: ess-scenario/1\n\
         domain: notice.sink\n\
         scenario: a-post-is-pinned\n\
         summary: A board opens, a post is made, and the binding pins it.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: notice.sink.OpenBoard\n\
         \x20   input:\n\
         \x20     board_id: b1\n\
         \x20 - at: 2026-01-05T09:01:00Z\n\
         \x20   command: notice.source.Post\n\
         \x20   input:\n\
         \x20     board_id: b1\n\
         \x20     beat: first\n\
         assert:\n\
         \x20 - view: notice.sink.BoardState\n\
         \x20   contains: {board_id: b1, last: first}\n",
    );
    let with_components = replay(&declared, &laptop(), &scenario);
    assert_eq!(
        with_components.status.code(),
        Some(0),
        "{with_components:?}"
    );
    let without = replay(&implied, &laptop(), &scenario);
    assert_eq!(without.status.code(), Some(0), "{without:?}");
    assert_eq!(stdout(&without), stdout(&with_components));
    assert_lines(
        &stdout(&without),
        &[
            "timeline[0] notice.sink.OpenBoard: holds, outcome opened\n",
            "timeline[1] notice.source.Post: holds, outcome posted\n",
            "assert[0] notice.sink.BoardState: holds\n",
        ],
    );
}
