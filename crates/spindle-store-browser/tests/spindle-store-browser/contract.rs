//! The store contract the memory and SQLite stores pass (`spindle_kernel::contract`), run on the
//! IndexedDB store, each case in a database of its own. After each case the store is closed and
//! the database opened again: every instance the case left is read back as it was, with its
//! version.

use spindle_kernel::{Store, Stored, contract};
use spindle_store_browser::BrowserStore;
use wasm_bindgen_test::wasm_bindgen_test;

fn scans(store: &mut dyn Store) -> Vec<(&'static str, Vec<(serde_json::Value, Stored)>)> {
    contract::ENTITIES
        .iter()
        .map(|entity| (*entity, store.scan(entity).expect("the scan reads").rows))
        .collect()
}

#[wasm_bindgen_test]
async fn the_browser_store_passes_the_store_contract() {
    assert!(!contract::CASES.is_empty());
    for case in contract::CASES {
        let name = crate::fresh(case.name);
        let mut store = BrowserStore::open(&name).await.expect("IndexedDB opens");
        (case.run)(&mut store);
        let held = scans(&mut store);
        store.flush().await.expect("every write of the case lands");
        drop(store);
        let mut reopened = BrowserStore::open(&name)
            .await
            .expect("IndexedDB opens again");
        assert_eq!(
            scans(&mut reopened),
            held,
            "{}: the reopened database holds what the store held",
            case.name
        );
    }
}
