//! The reserved `spindle:log` streams of a component's log that are no published event: what
//! `Store::events_after` and `GET /api/events` read leaves them out.
//!
//! The egress cursor (`story:event-egress-webhook`): the store position up to which a served run
//! posted a component's events to one `[http.<name>]` call, saved in the component's own log and
//! read back after a restart.

use serde_json::{Map, json};
use spindle_kernel::{Change, Event, Row, SqliteStore, Store};

const TASK: &str = "tasks.board.Task";

fn opened(identity: &str) -> Change {
    let mut payload = Map::new();
    payload.insert("task_id".to_owned(), json!(identity));
    Change {
        entity: TASK.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "Open".to_owned(),
            fields: std::collections::BTreeMap::from([("title".to_owned(), json!("t"))]),
        },
        events: vec![Event {
            name: "tasks.board.TaskOpened".to_owned(),
            payload,
        }],
        scanned: Vec::new(),
    }
}

fn names(store: &mut dyn Store) -> Vec<(String, u64)> {
    store
        .events_after(0, usize::MAX)
        .expect("the events read")
        .into_iter()
        .map(|committed| (committed.event.name, committed.position))
        .collect()
}

#[test]
fn an_egress_cursor_is_no_published_event() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("board.sqlite");
    let first = {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        assert_eq!(
            store.egress_position("hook").expect("a cursor reads"),
            0,
            "no event was posted yet"
        );
        store.commit(&opened("t1")).expect("t1 commits");
        let committed = names(&mut store);
        assert_eq!(committed.len(), 1, "{committed:?}");
        let (_, first) = committed[0].clone();
        store
            .egress_posted("hook", first)
            .expect("the cursor is saved");
        store.commit(&opened("t2")).expect("t2 commits");
        let after = names(&mut store);
        assert_eq!(
            after
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["tasks.board.TaskOpened", "tasks.board.TaskOpened"],
            "the cursor is no event of the log: {after:?}"
        );
        assert!(after[1].1 > first, "{after:?}");
        let reading = store.reading().expect("a reading handle");
        let read = reading
            .events_after(first, usize::MAX)
            .expect("the reading handle reads");
        assert_eq!(read.len(), 1, "only t2 is after the cursor: {read:?}");
        assert_eq!(read[0].event.payload["task_id"], json!("t2"));
        first
    };

    let mut again = SqliteStore::open(&path).expect("the store opens again");
    assert_eq!(
        again.egress_position("hook").expect("the cursor reads"),
        first,
        "the cursor survives a restart"
    );
    assert_eq!(
        again
            .egress_position("other")
            .expect("another cursor reads"),
        0,
        "each call has a cursor of its own"
    );
    assert_eq!(names(&mut again).len(), 2);
}
