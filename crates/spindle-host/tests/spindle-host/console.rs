//! What a model console reads from a run: the events published so far, in order, and a reset
//! that empties every store of the run, on memory and on SQLite.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Two components: `board`, which a request reaches, and `ledger`, which only a binding reaches
/// and which no view reads.
fn fixture() -> PathBuf {
    root().join("crates/spindle-cli/tests/fixtures/ingress")
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

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"console-memory\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// Runs `check` once on a host over SQLite and once on a host over memory.
fn on_every_store(check: impl Fn(&str, &mut Host)) {
    for (name, environment) in [("sqlite", laptop()), ("memory", in_memory())] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let loaded = spindle_spec::load(&fixture()).expect("the fixture compiles");
        let mut host = Host::new(
            loaded.ir,
            &environment,
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("{name}: the host starts: {error}"));
        check(name, &mut host);
    }
}

fn open(host: &mut Host, task: &str, title: &str) {
    host.submit(
        "tasks.board.OpenTask",
        object(json!({"task_id": task, "title": title})),
    )
    .expect("the task opens");
}

/// Closes `task`; true when the ledger recorded the closure too.
fn close(host: &mut Host, task: &str) -> bool {
    let report = host
        .submit("tasks.board.CloseTask", object(json!({"task_id": task})))
        .expect("the task closes");
    report.failed.is_empty()
        && report.refused.is_empty()
        && report
            .ran
            .iter()
            .any(|ran| ran.command == "tasks.ledger.RecordClosure")
}

fn published(host: &mut Host) -> Vec<(String, String, String, Value)> {
    host.published()
        .expect("the events read")
        .iter()
        .map(|published| {
            (
                published.component.clone(),
                published.command.clone(),
                published.event.name.clone(),
                Value::Object(published.event.payload.clone()),
            )
        })
        .collect()
}

#[test]
fn the_host_keeps_every_event_published_in_order() {
    on_every_store(|name, host| {
        assert!(
            host.published().expect("the events read").is_empty(),
            "{name}"
        );
        open(host, "t1", "First");
        open(host, "t2", "Second");
        assert!(close(host, "t1"), "{name}");
        let refused = host.submit("tasks.board.CloseTask", object(json!({"task_id": "t1"})));
        assert!(refused.is_err(), "{name}: a closed task stays closed");
        let row = |component: &str, command: &str, event: &str, payload: Value| {
            (
                component.to_owned(),
                command.to_owned(),
                event.to_owned(),
                payload,
            )
        };
        assert_eq!(
            published(host),
            [
                row(
                    "board",
                    "tasks.board.OpenTask",
                    "tasks.board.TaskOpened",
                    json!({"task_id": "t1", "title": "First"})
                ),
                row(
                    "board",
                    "tasks.board.OpenTask",
                    "tasks.board.TaskOpened",
                    json!({"task_id": "t2", "title": "Second"})
                ),
                row(
                    "board",
                    "tasks.board.CloseTask",
                    "tasks.board.TaskClosed",
                    json!({"task_id": "t1"})
                ),
                row(
                    "ledger",
                    "tasks.ledger.RecordClosure",
                    "tasks.ledger.ClosureRecorded",
                    json!({"task_id": "t1"})
                ),
            ],
            "{name}"
        );
    });
}

#[test]
fn a_reset_empties_every_store_and_the_published_events() {
    on_every_store(|name, host| {
        open(host, "t1", "First");
        assert!(close(host, "t1"), "{name}");
        open(host, "t2", "Second");
        assert_eq!(host.list("tasks.board.AllTasks").expect("lists").len(), 2);

        let emptied = host
            .reset()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(emptied, ["board", "ledger"], "{name}");
        assert!(
            host.published().expect("the events read").is_empty(),
            "{name}"
        );
        for view in ["tasks.board.AllTasks", "tasks.board.OpenTasks"] {
            assert_eq!(
                host.list(view).expect("lists"),
                Vec::<Map<String, Value>>::new(),
                "{name}: {view}"
            );
        }

        open(host, "t1", "Again");
        assert!(
            close(host, "t1"),
            "{name}: the ledger, which no view reads, holds no closure of t1 after the reset"
        );
        assert_eq!(
            host.list("tasks.board.AllTasks").expect("lists"),
            [object(
                json!({"task_id": "t1", "state": "Done", "title": "Again"})
            )],
            "{name}"
        );
        assert_eq!(published(host).len(), 3, "{name}");
    });
}

#[test]
fn without_a_reset_an_open_task_cannot_be_opened_again() {
    on_every_store(|name, host| {
        open(host, "t1", "First");
        assert!(close(host, "t1"), "{name}");
        assert!(
            host.submit(
                "tasks.board.OpenTask",
                object(json!({"task_id": "t1", "title": "Again"}))
            )
            .is_err(),
            "{name}: the probe the reset test relies on: t1 is still held"
        );
    });
}
