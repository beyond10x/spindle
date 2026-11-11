//! `GET /api/events` answered from the events each component's store committed
//! (`story:events-read-from-store`): the same after a restart on SQLite, in the order the host
//! published them, empty after a reset, and read beside the host's thread through each store's
//! reading handle, so the host keeps no list of the events of its own.
//!
//! The console fixture declares two components: `board`, which a request reaches, and `ledger`,
//! which only a binding reaches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Answer, console};
use spindle_kernel::{Change, MemoryStore, Origin, Row, SqliteStore, Store};

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

fn bound(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"events-{store}\"\n",
            "os = \"linux\"\n",
            "arch = \"x86_64\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "clock = [\"system\"]\n",
            "timer = [\"tokio\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
            "\"ingress.network\" = [\"http-loopback\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

fn tasks() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the console fixture compiles")
        .ir
}

/// Each host the console tests run the script on: over SQLite in `state`, over memory, and over
/// stores it was given, which a reset empties in place.
fn hosts(state: &Path) -> Vec<(&'static str, Host)> {
    let clock = || Arc::new(ManualClock::new());
    let sqlite = Host::new(tasks(), &laptop(), clock(), state).expect("the SQLite host opens");
    let memory = Host::new(tasks(), &bound("memory"), clock(), state).expect("the memory host");
    let given: Stores = ["board", "ledger"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                Box::new(MemoryStore::default()) as Box<dyn Store>,
            )
        })
        .collect();
    let supplied = Host::supplied(tasks(), &bound("elsewhere"), clock(), None, given)
        .expect("the host on given stores opens");
    vec![
        ("sqlite", sqlite),
        ("memory", memory),
        ("supplied", supplied),
    ]
}

fn submit(host: &mut Host, command: &str, input: Value) {
    let report = host
        .submit(command, object(input))
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// The console tests' script: two tasks opened, the first closed, which the ledger records, and
/// a second close of it refused.
fn script(host: &mut Host) {
    submit(
        host,
        "tasks.board.OpenTask",
        json!({"task_id": "t1", "title": "First"}),
    );
    submit(
        host,
        "tasks.board.OpenTask",
        json!({"task_id": "t2", "title": "Second"}),
    );
    submit(host, "tasks.board.CloseTask", json!({"task_id": "t1"}));
    assert!(
        host.submit("tasks.board.CloseTask", object(json!({"task_id": "t1"})))
            .is_err(),
        "a closed task stays closed"
    );
}

fn event(component: &str, command: &str, event: &str, payload: Value) -> Value {
    json!({"component": component, "command": command, "event": event, "payload": payload})
}

/// What the host published for [`script`], in order, as `GET /api/events` answered it from the
/// host's own list.
fn published() -> Vec<Value> {
    vec![
        event(
            "board",
            "tasks.board.OpenTask",
            "tasks.board.TaskOpened",
            json!({"task_id": "t1", "title": "First"}),
        ),
        event(
            "board",
            "tasks.board.OpenTask",
            "tasks.board.TaskOpened",
            json!({"task_id": "t2", "title": "Second"}),
        ),
        event(
            "board",
            "tasks.board.CloseTask",
            "tasks.board.TaskClosed",
            json!({"task_id": "t1"}),
        ),
        event(
            "ledger",
            "tasks.ledger.RecordClosure",
            "tasks.ledger.ClosureRecorded",
            json!({"task_id": "t1"}),
        ),
    ]
}

/// The events an answer of `GET /api/events` lists.
fn listed(what: &str, answer: Answer) -> Vec<Value> {
    assert_eq!(answer.status, 200, "{what}: {:?}", answer.body);
    let body = answer.body.unwrap_or_else(|| panic!("{what}: a body"));
    body["events"]
        .as_array()
        .unwrap_or_else(|| panic!("{what}: {{\"events\": [...]}}, got {body}"))
        .clone()
}

/// What `GET /api/events` answers on the host's thread and through its reader beside it, which
/// must agree.
fn answered(what: &str, host: &mut Host) -> Vec<Value> {
    let on_thread = listed(what, console::events(host));
    let reader = host.reader();
    let beside = std::thread::spawn(move || console::events_read(&reader))
        .join()
        .expect("the read beside the host's thread ends");
    assert_eq!(
        listed(what, beside),
        on_thread,
        "{what}: the read beside the host's thread answers as the host's thread"
    );
    on_thread
}

#[test]
fn events_keep_the_published_order() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    for (name, mut host) in hosts(state.path()) {
        assert_eq!(answered(name, &mut host), Vec::<Value>::new(), "{name}");
        script(&mut host);
        assert_eq!(answered(name, &mut host), published(), "{name}");
    }
}

#[test]
fn a_reset_empties_the_events() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    for (name, mut host) in hosts(state.path()) {
        script(&mut host);
        let reset = console::reset(&mut host, b"", None);
        assert_eq!(reset.status, 200, "{name}: {:?}", reset.body);
        assert_eq!(answered(name, &mut host), Vec::<Value>::new(), "{name}");
        submit(
            &mut host,
            "tasks.board.OpenTask",
            json!({"task_id": "t1", "title": "Again"}),
        );
        assert_eq!(
            answered(name, &mut host),
            [event(
                "board",
                "tasks.board.OpenTask",
                "tasks.board.TaskOpened",
                json!({"task_id": "t1", "title": "Again"}),
            )],
            "{name}: only what was published since the reset"
        );
    }
}

#[test]
fn events_survive_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let rally = || {
        spindle_spec::load(&root().join("examples/rally/ess"))
            .expect("rally compiles")
            .ir
    };
    let open = || {
        Host::new(
            rally(),
            &laptop(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .expect("laptop admits rally")
    };
    let mut host = open();
    submit(
        &mut host,
        "rally.table.OpenMatch",
        json!({"match_id": "m1", "target_score": 3}),
    );
    for side in ["West", "East"] {
        submit(
            &mut host,
            "rally.table.MovePaddle",
            json!({"match_id": "m1", "side": side, "step": "Up"}),
        );
    }
    let before = answered("before the restart", &mut host);
    let commands = before
        .iter()
        .map(|event| event["command"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        [
            "rally.table.OpenMatch",
            "rally.east.ObserveBall",
            "rally.west.ObserveBall",
            "rally.table.MovePaddle",
            "rally.table.MovePaddle"
        ],
        "the three commands' events, the ball each player saw after the match opened among \
         them, across three components' stores: {before:?}"
    );
    drop(host);

    let mut again = open();
    assert_eq!(
        answered("after the restart", &mut again),
        before,
        "the same events in the same order"
    );
}

/// The host keeps no list of the events it published: an event a second handle on the board's
/// SQLite file commits behind the host's back is answered too, read beside the host's thread
/// through the store's reading handle.
#[test]
fn the_events_are_read_from_the_store_not_from_a_list_the_host_keeps() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(
        tasks(),
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the SQLite host opens");
    submit(
        &mut host,
        "tasks.board.OpenTask",
        json!({"task_id": "t1", "title": "First"}),
    );

    let mut behind = SqliteStore::open(&state.path().join("board.sqlite"))
        .expect("a second handle on the board's file opens");
    behind.attribute(Some(Origin {
        command: "tasks.board.OpenTask".to_owned(),
        sequence: u64::MAX,
    }));
    let opened = event(
        "board",
        "tasks.board.OpenTask",
        "tasks.board.TaskOpened",
        json!({"task_id": "t9", "title": "Behind"}),
    );
    behind
        .commit(&Change {
            entity: "tasks.board.Task".to_owned(),
            identity: json!("t9"),
            expected: None,
            row: Row {
                state: "Open".to_owned(),
                fields: BTreeMap::from([("title".to_owned(), json!("Behind"))]),
            },
            events: vec![spindle_kernel::Event {
                name: "tasks.board.TaskOpened".to_owned(),
                payload: object(json!({"task_id": "t9", "title": "Behind"})),
            }],
            scanned: Vec::new(),
        })
        .expect("the second handle commits");
    drop(behind);

    let answer = answered("an event committed behind the host", &mut host);
    assert_eq!(answer.len(), 2, "{answer:?}");
    assert_eq!(answer[1], opened, "the event the store holds is answered");
}
