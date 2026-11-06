//! A run's state in IndexedDB outlives the store that wrote it: commands run on one handle, the
//! handle is dropped as a page reload drops it, and a new handle on the same database reads every
//! effect back, a removed book included.

use serde_json::{Map, Value, json};
use spindle_kernel::{Executed, NoExternals, Store, execute};
use spindle_spec::EssIr;
use spindle_store_browser::BrowserStore;
use wasm_bindgen_test::wasm_bindgen_test;

const SYSTEM: &str = include_str!("../fixtures/library/system.yaml");
const BOOKS: &str = include_str!("../fixtures/library/domains/books.yaml");
const BOOK: &str = "library.books.Book";

fn ir() -> EssIr {
    spindle_spec::compile(
        &["system.yaml".to_owned(), "domains/books.yaml".to_owned()],
        &[SYSTEM.to_owned(), BOOKS.to_owned()],
    )
    .expect("the library fixture compiles in wasm")
}

fn run(ir: &EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    let Value::Object(input) = input else {
        panic!("an object input")
    };
    execute(ir, store, &mut NoExternals, command, &input)
        .unwrap_or_else(|error| panic!("{command} runs: {error}"))
}

fn shelve(ir: &EssIr, store: &mut dyn Store, id: &str, title: &str) -> Executed {
    run(
        ir,
        store,
        "library.books.ShelveBook",
        json!({"book_id": id, "title": title}),
    )
}

fn payload(executed: &Executed) -> Vec<(String, Map<String, Value>)> {
    executed
        .events
        .iter()
        .map(|event| (event.name.clone(), event.payload.clone()))
        .collect()
}

#[wasm_bindgen_test]
async fn state_survives_a_reload() {
    let ir = ir();
    let name = crate::fresh("reload");
    let mut emitted = Vec::new();
    let (lent, removed) = {
        let mut store = BrowserStore::open(&name).await.expect("IndexedDB opens");
        emitted.extend(payload(&shelve(&ir, &mut store, "b1", "Tides")));
        emitted.extend(payload(&shelve(&ir, &mut store, "b2", "Field Notes")));
        let lend = run(
            &ir,
            &mut store,
            "library.books.LendBook",
            json!({"book_id": "b1"}),
        );
        assert_eq!(lend.outcome, "lent");
        emitted.extend(payload(&lend));
        let lent = store
            .load(BOOK, &json!("b1"))
            .expect("reads")
            .expect("b1 is held");
        let removed = store
            .load(BOOK, &json!("b2"))
            .expect("reads")
            .expect("b2 is held")
            .version;
        let remove = run(
            &ir,
            &mut store,
            "library.books.RemoveBook",
            json!({"book_id": "b2"}),
        );
        assert_eq!(remove.outcome, "removed");
        emitted.extend(payload(&remove));
        (lent, removed)
    };

    let mut store = BrowserStore::open(&name)
        .await
        .expect("IndexedDB opens after the reload");
    assert_eq!(
        store.load(BOOK, &json!("b1")).expect("reads"),
        Some(lent.clone()),
        "the lent book reads back as it rested, with its version"
    );
    assert_eq!(store.load(BOOK, &json!("b2")).expect("reads"), None);
    let again = run(
        &ir,
        &mut store,
        "library.books.LendBook",
        json!({"book_id": "b1"}),
    );
    assert_eq!(
        again.outcome, "wrong-state",
        "the command reads the state the reload kept"
    );
    let gone = run(
        &ir,
        &mut store,
        "library.books.RemoveBook",
        json!({"book_id": "b2"}),
    );
    assert_eq!(
        gone.outcome, "no-such-book",
        "the removal outlives the reload"
    );
    let reshelved = shelve(&ir, &mut store, "b2", "Field Notes, second edition");
    assert_eq!(reshelved.outcome, "shelved");
    emitted.extend(payload(&reshelved));
    let back = store
        .load(BOOK, &json!("b2"))
        .expect("reads")
        .expect("b2 is held again");
    assert!(
        back.version > removed,
        "a book shelved again goes on past the version it was removed at ({} after {removed})",
        back.version
    );
    store.flush().await.expect("the writes land");

    let recorded: Vec<(String, Map<String, Value>)> = store
        .events()
        .await
        .expect("the events read")
        .into_iter()
        .map(|event| (event.name, event.payload))
        .collect();
    assert_eq!(
        recorded, emitted,
        "every event the commands emitted is recorded, in commit order, across the reload"
    );
}
