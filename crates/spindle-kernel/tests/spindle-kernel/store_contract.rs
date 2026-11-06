//! The store contract (`spindle_kernel::contract`, story browser-store) run on the memory store
//! and on SQLite, each case on an empty store of its own.

use spindle_kernel::{MemoryStore, SqliteStore, contract};

#[test]
fn the_memory_store_passes_the_store_contract() {
    assert!(!contract::CASES.is_empty());
    for case in contract::CASES {
        (case.run)(&mut MemoryStore::default());
    }
}

#[test]
fn the_sqlite_store_passes_the_store_contract() {
    for case in contract::CASES {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let mut store =
            SqliteStore::open(&directory.path().join("contract.sqlite")).expect("SQLite opens");
        (case.run)(&mut store);
    }
}
