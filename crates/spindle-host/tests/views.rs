//! List views: every instance of the source, kept by a `state == <State>` filter and ranked by
//! `order_by`, read from the store of the component that owns the source, on memory and SQLite.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/views")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// The shipped laptop environment, which binds `store.durable` to `sqlite`.
fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"views-memory\"\n",
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
    ))
    .expect("the in-memory environment parses")
}

/// Runs `check` once on a host over SQLite and once on a host over memory.
fn on_every_store(check: impl Fn(&str, &mut Host)) {
    for (name, environment) in [("sqlite", laptop()), ("memory", in_memory())] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let loaded = spindle_spec::load(&fixture()).expect("the views fixture compiles");
        let mut host = Host::new(
            loaded.ir,
            &environment,
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .unwrap_or_else(|error| panic!("{name} admits the views fixture: {error}"));
        check(name, &mut host);
    }
}

fn open(host: &mut Host, task_id: &str, title: &str, opened_at: &str) {
    host.submit(
        "tasks.board.OpenTask",
        object(json!({"task_id": task_id, "title": title, "opened_at": opened_at})),
    )
    .unwrap_or_else(|error| panic!("{task_id} opens: {error}"));
}

fn close(host: &mut Host, task_id: &str) {
    host.submit("tasks.board.CloseTask", object(json!({"task_id": task_id})))
        .unwrap_or_else(|error| panic!("{task_id} closes: {error}"));
}

fn ids(rows: &[Map<String, Value>]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.get("task_id")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("a row without a task_id: {row:?}"))
                .to_owned()
        })
        .collect()
}

#[test]
fn a_list_view_answers_every_instance() {
    on_every_store(|store, host| {
        assert_eq!(
            host.list("tasks.board.AllTasks").expect("the list answers"),
            Vec::<Map<String, Value>>::new(),
            "{store}: a list over no instances is empty"
        );
        open(host, "t3", "third", "2026-01-03T00:00:00Z");
        open(host, "t1", "first", "2026-01-01T00:00:00Z");
        open(host, "t2", "second", "2026-01-02T00:00:00Z");
        close(host, "t2");
        let rows = host.list("tasks.board.AllTasks").expect("the list answers");
        assert_eq!(
            rows.into_iter().map(Value::Object).collect::<Vec<_>>(),
            vec![
                json!({"task_id": "t1", "state": "Open", "title": "first"}),
                json!({"task_id": "t2", "state": "Done", "title": "second"}),
                json!({"task_id": "t3", "state": "Open", "title": "third"}),
            ],
            "{store}: every instance, each projected, ranked by identity where the view declares no order"
        );
    });
}

#[test]
fn a_state_filter_keeps_only_instances_in_that_state() {
    on_every_store(|store, host| {
        open(host, "t1", "first", "2026-01-01T00:00:00Z");
        open(host, "t2", "second", "2026-01-02T00:00:00Z");
        open(host, "t3", "third", "2026-01-03T00:00:00Z");
        close(host, "t2");
        let rows = host
            .list("tasks.board.OpenTasks")
            .expect("the list answers");
        assert_eq!(
            ids(&rows),
            ["t1", "t3"],
            "{store}: `state == Open` keeps the open tasks only"
        );
        assert!(
            rows.iter()
                .all(|row| row.get("state") == Some(&json!("Open"))),
            "{store}: every kept row is Open: {rows:?}"
        );
        assert_eq!(
            host.view("tasks.board.OpenTasks", &json!("t2"))
                .expect("the view answers"),
            None,
            "{store}: a closed task is not in the open view, read by identity either"
        );
        assert_eq!(
            host.view("tasks.board.OpenTasks", &json!("t1"))
                .expect("the view answers")
                .map(Value::Object),
            Some(json!({"task_id": "t1", "state": "Open", "title": "first"})),
            "{store}: an open task is in the open view by identity"
        );
        close(host, "t1");
        assert_eq!(
            ids(&host
                .list("tasks.board.OpenTasks")
                .expect("the list answers")),
            ["t3"],
            "{store}: read_your_writes: the list reflects the close that just returned"
        );
    });
}

/// `opened_at` is a `Timestamp`, so rows rank by the instant each names, not by its spelling:
/// `12:00:00+01:00` is half an hour before `11:30:00Z`. `t2` and `t4` name one instant, so they
/// tie, and the tie is broken by identity.
#[test]
fn order_by_desc_on_a_timestamp_orders_newest_first() {
    on_every_store(|store, host| {
        open(host, "t1", "eleven", "2026-01-01T12:00:00+01:00");
        open(host, "t2", "half past", "2026-01-01T11:30:00Z");
        open(host, "t3", "next day", "2026-01-02T00:00:00Z");
        open(host, "t4", "half past again", "2026-01-01T12:30:00+01:00");
        let rows = host
            .list("tasks.board.NewestTasks")
            .expect("the eventual list answers from the same store");
        assert_eq!(
            ids(&rows),
            ["t3", "t2", "t4", "t1"],
            "{store}: newest first by instant, ties by identity: {rows:?}"
        );
        assert_eq!(
            rows.first().map(|row| Value::Object(row.clone())),
            Some(
                json!({"task_id": "t3", "title": "next day", "opened_at": "2026-01-02T00:00:00Z"})
            ),
            "{store}: each row is the view's projection"
        );
    });
}

#[test]
fn a_filter_other_than_a_state_comparison_stays_unsupported_and_named() {
    on_every_store(|store, host| {
        open(host, "t1", "Chores", "2026-01-01T00:00:00Z");
        match host.list("tasks.board.ChoreTasks") {
            Err(HostError::Unsupported(text)) => assert!(
                text.contains("tasks.board.ChoreTasks") && text.contains("title == Chores"),
                "{store}: the refusal names the view and its filter: {text}"
            ),
            other => panic!("{store}: `title == Chores` is not refused as unsupported: {other:?}"),
        }
        match host.view("tasks.board.ChoreTasks", &json!("t1")) {
            Err(HostError::Unsupported(text)) => assert!(
                text.contains("title == Chores"),
                "{store}: a one-row read of it is refused the same way: {text}"
            ),
            other => panic!("{store}: a one-row read is not refused as unsupported: {other:?}"),
        }
    });
}

#[test]
fn a_view_with_parameters_answers_the_rows_its_values_select() {
    on_every_store(|store, host| {
        open(host, "t1", "Chores", "2026-01-01T00:00:00Z");
        open(host, "t2", "Errands", "2026-01-02T00:00:00Z");
        open(host, "t3", "Chores", "2026-01-03T00:00:00Z");
        let rows = host
            .list_with(
                "tasks.board.TasksTitled",
                &object(json!({"wanted": "Chores"})),
            )
            .unwrap_or_else(|error| panic!("{store}: the titled view is answered: {error}"));
        assert_eq!(ids(&rows), ["t1", "t3"], "{store}");
        let rows = host
            .list_with(
                "tasks.board.TasksTitled",
                &object(json!({"wanted": "Nothing"})),
            )
            .unwrap_or_else(|error| panic!("{store}: the titled view is answered: {error}"));
        assert!(rows.is_empty(), "{store}: {rows:?}");
    });
}

#[test]
fn a_view_read_without_its_required_parameter_is_refused_naming_it() {
    on_every_store(|store, host| {
        match host.list("tasks.board.TasksTitled") {
            Err(HostError::Parameters { view, detail }) => assert!(
                view == "tasks.board.TasksTitled" && detail.contains("wanted"),
                "{store}: the refusal names the view and the parameter: {view}: {detail}"
            ),
            other => panic!("{store}: a read without `wanted` is not refused: {other:?}"),
        }
        match host.list_with("tasks.board.TasksTitled", &object(json!({"wanted": 3}))) {
            Err(HostError::Parameters { detail, .. }) => assert!(
                detail.contains("wanted"),
                "{store}: a value of the wrong type is refused naming it: {detail}"
            ),
            other => panic!("{store}: `wanted: 3` is not refused: {other:?}"),
        }
        match host.list_with("tasks.board.TasksTitled", &object(json!({"other": "x"}))) {
            Err(HostError::Parameters { detail, .. }) => assert!(
                detail.contains("other"),
                "{store}: an undeclared parameter is refused naming it: {detail}"
            ),
            other => panic!("{store}: `other` is not refused: {other:?}"),
        }
    });
}

#[test]
fn a_list_of_an_undeclared_view_is_no_view() {
    on_every_store(|store, host| {
        assert!(
            matches!(host.list("tasks.board.Nothing"), Err(HostError::NoView(name)) if name == "tasks.board.Nothing"),
            "{store}: an undeclared view is NoView"
        );
    });
}
