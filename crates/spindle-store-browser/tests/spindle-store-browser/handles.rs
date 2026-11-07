//! Several handles on one database answer as one store: a change naming a version another handle
//! moved past is a conflict naming the newer version, as `MemoryStore` answers one handle. Handles
//! of one global scope share what they hold; a handle opened apart stands for another tab.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Event, Row, Store, StoreError};
use spindle_store_browser::BrowserStore;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

const ENTITY: &str = "handles.tasks.Task";

fn change(identity: &Value, expected: Option<u64>, step: u64) -> Change {
    Change {
        entity: ENTITY.to_owned(),
        identity: identity.clone(),
        expected,
        row: Row {
            state: "Open".to_owned(),
            fields: BTreeMap::from([("step".to_owned(), json!(step))]),
        },
        events: vec![Event {
            name: "handles.tasks.Touched".to_owned(),
            payload: Map::from_iter([("step".to_owned(), json!(step))]),
        }],
    }
}

fn conflict(result: Result<(), StoreError>) -> Option<(Option<u64>, Option<u64>)> {
    match result {
        Ok(()) => None,
        Err(StoreError::Conflict {
            expected, actual, ..
        }) => Some((expected, actual)),
        Err(other) => panic!("{other}"),
    }
}

#[wasm_bindgen_test]
async fn a_second_handle_changing_a_moved_instance_is_a_conflict() {
    let name = crate::fresh("handles-moved");
    let identity = json!("t1");
    let mut first = BrowserStore::open(&name).await.expect("IndexedDB opens");
    first
        .commit(&change(&identity, None, 1))
        .expect("the creation commits");
    first.flush().await.expect("lands");
    let mut second = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens twice");
    assert_eq!(
        second
            .load(ENTITY, &identity)
            .expect("loads")
            .map(|held| held.version),
        Some(1),
        "the second handle reads the creation"
    );
    first
        .commit(&change(&identity, Some(1), 2))
        .expect("the first handle moves the instance to version 2");
    first.flush().await.expect("lands");
    assert_eq!(
        conflict(second.commit(&change(&identity, Some(1), 3))),
        Some((Some(1), Some(2))),
        "a change naming the version the other handle moved past is a conflict naming the newer"
    );
    second.flush().await.expect("nothing failed to land");
    drop(first);
    drop(second);
    let mut reopened = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens again");
    let held = reopened
        .load(ENTITY, &identity)
        .expect("loads")
        .expect("the instance is held");
    assert_eq!(
        (held.version, held.row.fields.get("step")),
        (2, Some(&json!(2))),
        "the database holds the first handle's change"
    );
}

/// Resolves after `milliseconds`, so the browser delivers what it has queued.
async fn pause(milliseconds: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let set_timeout = js_sys::Reflect::get(&js_sys::global(), &"setTimeout".into())
            .expect("setTimeout exists")
            .unchecked_into::<js_sys::Function>();
        set_timeout
            .call2(&JsValue::UNDEFINED, &resolve, &milliseconds.into())
            .expect("setTimeout schedules");
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("the pause resolves");
}

/// A handle in another global scope (another tab) that has not yet heard of a write: its stale
/// creation answers as its copy says, but the transaction finds the stored row and the write
/// does not land; the database keeps the first creation.
#[wasm_bindgen_test]
async fn a_stale_write_from_another_scope_does_not_land() {
    let name = crate::fresh("handles-stale");
    let identity = json!("t1");
    let mut here = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut there = BrowserStore::open_apart(&name)
        .await
        .expect("IndexedDB opens in another scope");
    here.commit(&change(&identity, None, 1))
        .expect("the creation commits");
    here.flush().await.expect("lands");
    there
        .commit(&change(&identity, None, 2))
        .expect("the stale handle's copy holds no instance");
    let refused = there
        .flush()
        .await
        .expect_err("the stale write does not land");
    assert!(
        refused.to_string().contains("another handle wrote"),
        "{refused}"
    );
    assert!(
        there.load(ENTITY, &identity).is_err(),
        "the stale handle answers nothing more"
    );
    drop(here);
    drop(there);
    let mut reopened = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens again");
    let held = reopened
        .load(ENTITY, &identity)
        .expect("loads")
        .expect("the instance is held");
    assert_eq!(
        (held.version, held.row.fields.get("step")),
        (1, Some(&json!(1))),
        "the database holds the first creation"
    );
    assert_eq!(
        reopened.events().await.expect("reads").len(),
        1,
        "only the landed write's event is recorded"
    );
}

/// A landed write is announced to the handles of other global scopes, which then answer from
/// it: a change at the announced version commits there, and is heard back here.
#[wasm_bindgen_test]
async fn a_landed_write_reaches_a_handle_in_another_scope() {
    let name = crate::fresh("handles-announced");
    let identity = json!("t1");
    let mut here = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut there = BrowserStore::open_apart(&name)
        .await
        .expect("IndexedDB opens in another scope");
    here.commit(&change(&identity, None, 1))
        .expect("the creation commits");
    here.flush().await.expect("lands");
    let mut heard = None;
    for _ in 0..50 {
        heard = there.load(ENTITY, &identity).expect("loads");
        if heard.is_some() {
            break;
        }
        pause(20).await;
    }
    assert_eq!(
        heard.map(|held| held.version),
        Some(1),
        "the other scope hears the creation"
    );
    assert_eq!(
        conflict(there.commit(&change(&identity, None, 2))),
        Some((None, Some(1))),
        "a second creation there is a conflict"
    );
    there
        .commit(&change(&identity, Some(1), 3))
        .expect("a change at the heard version commits");
    there.flush().await.expect("the change lands");
    let mut back = None;
    for _ in 0..50 {
        back = here
            .load(ENTITY, &identity)
            .expect("loads")
            .map(|held| held.version);
        if back == Some(2) {
            break;
        }
        pause(20).await;
    }
    assert_eq!(back, Some(2), "the change is heard back here");
}
