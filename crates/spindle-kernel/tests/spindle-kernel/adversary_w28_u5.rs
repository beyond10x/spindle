//! Adversary, wave 28 unit U5 (`story:events-record-their-caller`): what the SQLite store writes
//! into the event log for a write attributed to a cause, read from the log itself rather than
//! back through `Store::events_after`: the append's actor, a request id of its own for each
//! append, and the origin's `cause` in the JSON shape `spindle_kernel::Provenance` documents.

use eventlog_core::{EventStore, RecordedEvent, TenantId};
use eventlog_sqlite::SqliteEventStore;
use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Event, Origin, Provenance, Row, Sender, SqliteStore, Store};

const TASK: &str = "adversary.tasks.Task";

fn change(identity: &str, event: &str) -> Change {
    Change {
        entity: TASK.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "Open".to_owned(),
            fields: std::collections::BTreeMap::from([("id".to_owned(), json!(identity))]),
        },
        events: vec![named(event)],
        scanned: Vec::new(),
    }
}

fn named(event: &str) -> Event {
    Event {
        name: format!("adversary.tasks.{event}"),
        payload: Map::new(),
    }
}

fn caused(sequence: u64, cause: Option<Provenance>) -> Option<Origin> {
    Some(Origin {
        command: "adversary.tasks.Act".to_owned(),
        sequence,
        cause,
    })
}

fn owner() -> Provenance {
    Provenance::Request(Sender {
        actor: Some("adversary.tasks.Owner".to_owned()),
        attributes: Map::from_iter([("team".to_owned(), json!("blue"))]),
        request: Some("r-1".to_owned()),
    })
}

/// Every event of the tenant's log, as the event log answers them.
fn logged(path: &std::path::Path) -> Vec<RecordedEvent> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let log = runtime
        .block_on(SqliteEventStore::open(
            path.to_str().expect("a UTF-8 path"),
            "spindle",
        ))
        .expect("the file opens");
    let tenant = TenantId::new("local").expect("a tenant");
    runtime
        .block_on(log.read_feed(&tenant, 0, 10_000))
        .expect("the feed reads")
        .events
}

/// The append of a write attributed to a request's or a seed entry's caller, or to a binding
/// whose trigger names one, records that caller's actor; a tick's, an unknown trigger's and an
/// unattributed write's record the runtime. Each append keeps a request id of its own.
#[test]
fn the_append_records_the_actor_of_its_cause() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("state.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        store.attribute(caused(1, Some(owner())));
        store.commit(&change("t1", "A")).expect("A commits");
        store.attribute(caused(
            2,
            Some(Provenance::Seed {
                digest: "sha256:5eed".to_owned(),
                sender: Sender {
                    actor: Some("adversary.tasks.Operator".to_owned()),
                    attributes: Map::new(),
                    request: Some("seed-1".to_owned()),
                },
            }),
        ));
        store.commit(&change("t2", "B")).expect("B commits");
        store.attribute(caused(
            3,
            Some(Provenance::Binding {
                binding: "follow".to_owned(),
                trigger: Some(Box::new(owner())),
            }),
        ));
        store.commit_events(&[named("C")], &[]).expect("C commits");
        store.attribute(caused(
            4,
            Some(Provenance::Tick {
                binding: "every-minute".to_owned(),
            }),
        ));
        store.commit(&change("t3", "D")).expect("D commits");
        store.attribute(caused(
            5,
            Some(Provenance::Binding {
                binding: "follow".to_owned(),
                trigger: None,
            }),
        ));
        store.commit(&change("t4", "E")).expect("E commits");
        store.attribute(None);
        store.commit(&change("t5", "F")).expect("F commits");
    }
    let log = logged(&path);
    let domain: Vec<&RecordedEvent> = log
        .iter()
        .filter(|event| event.name.starts_with("adversary.tasks."))
        .collect();
    let actors: Vec<(&str, &str)> = domain
        .iter()
        .map(|event| (event.name.as_str(), event.actor.as_str()))
        .collect();
    assert_eq!(
        actors,
        [
            ("adversary.tasks.A", "adversary.tasks.Owner"),
            ("adversary.tasks.B", "adversary.tasks.Operator"),
            ("adversary.tasks.C", "adversary.tasks.Owner"),
            ("adversary.tasks.D", "spindle-kernel"),
            ("adversary.tasks.E", "spindle-kernel"),
            ("adversary.tasks.F", "spindle-kernel"),
        ]
    );
    let requests: std::collections::BTreeSet<&str> = domain
        .iter()
        .map(|event| event.request_id.as_str())
        .collect();
    assert_eq!(requests.len(), 6, "each append a request id of its own");
    assert!(
        !requests.contains("r-1"),
        "the caller's request id stays in the origin, not the log's column"
    );
}

/// The `origin` a `spindle.written` event carries names its cause in the shape `Provenance`
/// documents: `{"kind": "request", "actor", "attributes", "request"}`, and for a binding
/// `{"kind": "binding", "binding", "trigger"}` with the trigger's own shape inside.
#[test]
fn the_written_origin_names_its_cause_as_documented() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("state.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        store.attribute(caused(1, Some(owner())));
        store.commit(&change("t1", "A")).expect("A commits");
        store.attribute(caused(
            2,
            Some(Provenance::Binding {
                binding: "follow".to_owned(),
                trigger: Some(Box::new(owner())),
            }),
        ));
        store.commit(&change("t2", "B")).expect("B commits");
    }
    let origins: Vec<Value> = logged(&path)
        .into_iter()
        .filter(|event| event.name == "spindle.written")
        .map(|event| event.data["origin"].clone())
        .collect();
    let request = json!({
        "kind": "request",
        "actor": "adversary.tasks.Owner",
        "attributes": {"team": "blue"},
        "request": "r-1",
    });
    assert_eq!(
        origins,
        [
            json!({"command": "adversary.tasks.Act", "sequence": 1, "cause": request}),
            json!({
                "command": "adversary.tasks.Act",
                "sequence": 2,
                "cause": {"kind": "binding", "binding": "follow", "trigger": request},
            }),
        ]
    );
}

/// A binding's trigger is never itself a binding's: a chain of bindings records the request that
/// started it, and a chain whose first trigger is not known records none.
#[test]
fn a_chain_of_bindings_records_the_request_that_started_it() {
    let first = Provenance::Binding {
        binding: "one".to_owned(),
        trigger: owner().trigger().map(Box::new),
    };
    let second = Provenance::Binding {
        binding: "two".to_owned(),
        trigger: first.trigger().map(Box::new),
    };
    assert_eq!(second.trigger(), Some(owner()));
    assert_eq!(
        second.sender().and_then(|sender| sender.actor.as_deref()),
        Some("adversary.tasks.Owner")
    );
    let unknown = Provenance::Binding {
        binding: "one".to_owned(),
        trigger: None,
    };
    let after = Provenance::Binding {
        binding: "two".to_owned(),
        trigger: unknown.trigger().map(Box::new),
    };
    assert_eq!(after.trigger(), None);
    assert_eq!(after.sender(), None);
}
