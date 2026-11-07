//! Adversary, story browser-store: the IndexedDB store against the memory store on one seeded
//! sequence of creations, changes and removals, with the handle dropped unflushed and the
//! database reopened at seeded points, as a reload drops it. Every answer, every held row and
//! version, and the recorded events must be the memory store's.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, Event, MemoryStore, Removal, Row, Store, StoreError, Stored};
use spindle_store_browser::BrowserStore;
use wasm_bindgen_test::wasm_bindgen_test;

const ENTITY: &str = "adversary.tasks.Task";

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
    let row = Row {
        state: "Open".to_owned(),
        fields: BTreeMap::from([("step".to_owned(), json!(step))]),
    };
    let events = vec![Event {
        name: "adversary.tasks.Touched".to_owned(),
        payload: Map::from_iter([("step".to_owned(), json!(step))]),
    }];
    answer(match op {
        0 => store.commit(&Change {
            entity: ENTITY.to_owned(),
            identity: identity.clone(),
            expected: None,
            row,
            events,
        }),
        1 => store.commit(&Change {
            entity: ENTITY.to_owned(),
            identity: identity.clone(),
            expected: Some(version),
            row,
            events,
        }),
        _ => store.remove(&Removal {
            entity: ENTITY.to_owned(),
            identity: identity.clone(),
            expected: version,
            events,
        }),
    })
}

fn held(store: &mut dyn Store) -> Vec<(Value, Stored)> {
    store.scan(ENTITY).expect("scans")
}

#[wasm_bindgen_test]
async fn the_browser_store_answers_a_seeded_sequence_as_the_memory_store_across_reopens() {
    let name = crate::fresh("adversary-sequence");
    let mut memory = MemoryStore::default();
    let mut browser = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let identities = [json!("a"), json!("b"), json!(1)];
    let mut random = Lcg(0x5eed_b10c);
    for step in 0..240 {
        let identity = &identities[random.below(3) as usize];
        let op = random.below(3);
        let version = random.below(8);
        let on_memory = apply(&mut memory, step, op, identity, version);
        let on_browser = apply(&mut browser, step, op, identity, version);
        assert_eq!(
            on_browser, on_memory,
            "step {step}: op {op} on {identity} naming {version}"
        );
        if random.below(10) == 0 {
            drop(browser);
            browser = BrowserStore::open(&name)
                .await
                .expect("IndexedDB opens again");
        }
        assert_eq!(held(&mut browser), held(&mut memory), "step {step}: rows");
    }
    let recorded = browser.events().await.expect("the events read");
    assert_eq!(
        recorded,
        memory.events(),
        "every event, in commit order, across the reopens"
    );
}

#[wasm_bindgen_test]
async fn events_read_without_a_flush_include_the_writes_made_so_far() {
    let name = crate::fresh("adversary-events");
    let mut store = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut memory = MemoryStore::default();
    for step in 0..5 {
        let identity = json!(format!("t{step}"));
        assert_eq!(apply(&mut store, step, 0, &identity, 0), Ok(()));
        assert_eq!(apply(&mut memory, step, 0, &identity, 0), Ok(()));
    }
    assert_eq!(store.events().await.expect("reads"), memory.events());
}

/// Two handles on one database: the second creation of one identity is a conflict, as the
/// `Store` contract (`a_second_creation_of_one_identity_is_a_conflict`) answers it on one handle.
/// Ignored until story:browser-store-several-tabs, which made the two handles answer as one store.
#[wasm_bindgen_test]
async fn a_second_handle_creating_a_held_identity_is_a_conflict() {
    let name = crate::fresh("adversary-two-handles");
    let mut first = BrowserStore::open(&name).await.expect("IndexedDB opens");
    let mut second = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens twice");
    let identity = json!("t1");
    assert_eq!(apply(&mut first, 1, 0, &identity, 0), Ok(()));
    first.flush().await.expect("lands");
    let again = apply(&mut second, 2, 0, &identity, 0);
    assert!(
        again.is_err(),
        "a second creation of a held identity is a conflict, whichever handle makes it"
    );
}
