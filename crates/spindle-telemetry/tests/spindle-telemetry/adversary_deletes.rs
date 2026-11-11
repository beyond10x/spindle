//! Adversary case for `CountingStore::remove` (story kernel-delete-outcomes): no other test calls
//! it, so a removal that recorded nothing, or recorded twice, stayed green.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, Removal, Store, StoreError};
use spindle_telemetry::{Counters, CountingStore, Sink, catalogue};

fn books() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-kernel/tests/fixtures/deletes")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn a_deleting_outcome_counts_one_row_written_and_its_events() {
    let loaded = spindle_spec::load(&books()).expect("the deletes fixture compiles");
    let ir = &loaded.ir;
    let counters = Arc::new(Counters::new(
        catalogue(ir, "catalogue").expect("the component exists"),
    ));
    let sink = Sink::new(counters.clone());
    let mut store = CountingStore::new(MemoryStore::default(), sink);
    let shelved = spindle_kernel::execute(
        ir,
        &mut store,
        &mut NoExternals,
        "library.books.ShelveBook",
        &object(json!({"book_id": "b1", "title": "Tides"})),
    )
    .expect("the book is shelved");
    assert_eq!(shelved.outcome, "shelved");
    let after_shelving = counters
        .value("store.bytes_written")
        .expect("bytes counted");
    let held = store
        .load("library.books.Book", &json!("b1"))
        .expect("reads")
        .expect("held");

    let removed = spindle_kernel::execute(
        ir,
        &mut store,
        &mut NoExternals,
        "library.books.RemoveBook",
        &object(json!({"book_id": "b1"})),
    )
    .expect("the book is removed");
    assert_eq!(removed.outcome, "removed");
    assert_eq!(counters.value("store.rows_written"), Some(2));
    assert_eq!(
        counters.value("event.library.books.BookRemoved.appended"),
        Some(1)
    );
    let event_bytes: u64 = removed
        .events
        .iter()
        .map(|event| serde_json::to_vec(event).expect("an event").len() as u64)
        .sum();
    assert!(event_bytes > 0);
    assert_eq!(
        counters.value("store.bytes_written"),
        Some(after_shelving + event_bytes),
        "a removal counts its events' bytes and no row"
    );

    let refused = store.remove(&Removal {
        entity: "library.books.Book".to_owned(),
        identity: json!("b1"),
        expected: held.version,
        events: removed.events.clone(),
        scanned: Vec::new(),
    });
    assert!(matches!(refused, Err(StoreError::Conflict { .. })));
    assert_eq!(
        counters.value("store.rows_written"),
        Some(2),
        "a refused removal counts nothing"
    );
    assert_eq!(
        counters.value("event.library.books.BookRemoved.appended"),
        Some(1)
    );
}
