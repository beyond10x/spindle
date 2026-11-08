//! Adversary, story browser-store-several-tabs: handles in two global scopes (one opened apart
//! stands for another tab) against the memory store, a failed handle and what a reopen in its
//! scope answers, `events` on a handle whose write did not land, and a removal racing a stale
//! change from another scope.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Event, MemoryStore, Removal, Row, Store, StoreError, Stored};
use spindle_store_browser::BrowserStore;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

const ENTITY: &str = "tabs.tasks.Task";

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

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
            name: "tabs.tasks.Touched".to_owned(),
            payload: Map::from_iter([("step".to_owned(), json!(step))]),
        }],
        scanned: Vec::new(),
    }
}

fn removal(identity: &Value, expected: u64, step: u64) -> Removal {
    Removal {
        entity: ENTITY.to_owned(),
        identity: identity.clone(),
        expected,
        events: vec![Event {
            name: "tabs.tasks.Removed".to_owned(),
            payload: Map::from_iter([("step".to_owned(), json!(step))]),
        }],
    }
}

fn answer(result: Result<(), StoreError>) -> Result<(), (Option<u64>, Option<u64>)> {
    match result {
        Ok(()) => Ok(()),
        Err(StoreError::Conflict {
            expected, actual, ..
        }) => Err((expected, actual)),
        Err(other) => panic!("{other}"),
    }
}

fn apply(
    store: &mut dyn Store,
    step: u64,
    op: u64,
    identity: &Value,
    version: u64,
) -> Result<(), (Option<u64>, Option<u64>)> {
    answer(match op {
        0 => store.commit(&change(identity, None, step)),
        1 => store.commit(&change(identity, Some(version), step)),
        _ => store.remove(&removal(identity, version, step)),
    })
}

fn held(store: &mut dyn Store) -> Vec<(Value, Stored)> {
    store.scan(ENTITY).expect("scans").rows
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

/// Waits, bounded, until `store` holds `expected`; returns what it holds then.
async fn heard(store: &mut BrowserStore, expected: &[(Value, Stored)]) -> Vec<(Value, Stored)> {
    let mut now = held(store);
    for _ in 0..50 {
        if now == expected {
            break;
        }
        pause(20).await;
        now = held(store);
    }
    now
}

/// Two scopes take turns on one seeded sequence of creations, changes and removals, each turn
/// flushed and heard by the other scope before the next: every answer, both scopes' rows, the
/// recorded events and a reopen must be the memory store's.
#[wasm_bindgen_test]
async fn two_scopes_taking_turns_answer_a_seeded_sequence_as_the_memory_store() {
    let name = crate::fresh("tabs-sequence");
    let mut memory = MemoryStore::default();
    let mut here = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut there = BrowserStore::open_apart(&name)
        .await
        .expect("IndexedDB opens in another scope");
    let identities = [json!("a"), json!("b"), json!(1)];
    let mut random = Lcg(0x7ab5_0b10);
    for step in 0..80 {
        let identity = &identities[random.below(3) as usize];
        let op = random.below(3);
        let version = random.below(5);
        let (acting, other) = if random.below(2) == 0 {
            (&mut here, &mut there)
        } else {
            (&mut there, &mut here)
        };
        let on_memory = apply(&mut memory, step, op, identity, version);
        let on_browser = apply(acting, step, op, identity, version);
        assert_eq!(
            on_browser, on_memory,
            "step {step}: op {op} on {identity} naming {version}"
        );
        acting.flush().await.expect("the turn lands");
        let expected = held(&mut memory);
        assert_eq!(
            held(acting),
            expected,
            "step {step}: the acting scope's rows"
        );
        assert_eq!(
            heard(other, &expected).await,
            expected,
            "step {step}: the other scope hears the turn"
        );
    }
    assert_eq!(
        here.events().await.expect("reads"),
        memory.events(),
        "every event, in commit order"
    );
    drop(here);
    drop(there);
    let mut reopened = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens again");
    assert_eq!(held(&mut reopened), held(&mut memory), "a reopen's rows");
}

/// A handle whose stale write did not land: the natural reopen, assigning a new `open` over the
/// failed handle, must read the database afresh, not hand back the failed copy the registry
/// still holds while the old handle is alive.
#[wasm_bindgen_test]
async fn a_reopen_over_a_handle_whose_write_did_not_land_reads_the_database_afresh() {
    let name = crate::fresh("tabs-reopen");
    let identity = json!("t1");
    let mut there = BrowserStore::open_apart(&name)
        .await
        .expect("IndexedDB opens in another scope");
    let mut here = BrowserStore::open(&name).await.expect("IndexedDB opens");
    there
        .commit(&change(&identity, None, 1))
        .expect("the creation commits");
    there.flush().await.expect("lands");
    here.commit(&change(&identity, None, 2))
        .expect("the stale copy holds no instance");
    here.flush()
        .await
        .expect_err("the stale write does not land");
    here = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens again");
    assert_eq!(
        here.load(ENTITY, &identity)
            .expect("a reopened handle answers")
            .map(|stored| stored.version),
        Some(1),
        "the reopened handle reads the other scope's creation"
    );
}

/// `events` promises the writes made so far: on a handle whose write is about to be refused it
/// must not answer `Ok` with a log that lacks that write.
#[wasm_bindgen_test]
async fn events_on_a_handle_whose_write_did_not_land_is_not_an_answer() {
    let name = crate::fresh("tabs-events");
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
        .expect("the stale copy holds no instance");
    let mut memory = MemoryStore::default();
    memory
        .commit(&change(&identity, None, 2))
        .expect("the memory store commits");
    match there.events().await {
        Err(_) => {}
        Ok(events) => assert_eq!(
            events,
            memory.events(),
            "an Ok answer holds the handle's own write, as the memory store's log does"
        ),
    }
}

/// A removal in one scope, heard nowhere yet, and a change to the removed instance from another
/// scope that still holds its row at the removed version: the change must not land, and the
/// instance stays removed, so a creation goes on from the removed version.
#[wasm_bindgen_test]
async fn a_stale_change_to_an_instance_removed_in_another_scope_does_not_land() {
    let name = crate::fresh("tabs-removed");
    let identity = json!("t1");
    let mut here = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut there = BrowserStore::open_apart(&name)
        .await
        .expect("IndexedDB opens in another scope");
    here.commit(&change(&identity, None, 1))
        .expect("the creation commits");
    here.flush().await.expect("lands");
    let created = held(&mut here);
    assert_eq!(heard(&mut there, &created).await, created, "heard there");
    here.remove(&removal(&identity, 1, 2))
        .expect("the removal commits");
    here.flush().await.expect("lands");
    there
        .commit(&change(&identity, Some(1), 3))
        .expect("the stale copy holds version 1");
    there
        .flush()
        .await
        .expect_err("the stale change does not land");
    drop(here);
    drop(there);
    let mut reopened = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens again");
    assert_eq!(
        reopened.load(ENTITY, &identity).expect("loads"),
        None,
        "the instance stays removed"
    );
    reopened
        .commit(&change(&identity, None, 4))
        .expect("a creation commits");
    assert_eq!(
        reopened
            .load(ENTITY, &identity)
            .expect("loads")
            .map(|stored| stored.version),
        Some(2),
        "the creation goes on from the removed version"
    );
    reopened.flush().await.expect("lands");
    assert_eq!(
        reopened.events().await.expect("reads").len(),
        3,
        "the creation, the removal and the second creation are recorded; the stale change is not"
    );
}
