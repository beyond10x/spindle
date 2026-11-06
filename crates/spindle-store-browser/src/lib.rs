//! A [`Store`](spindle_kernel::Store) in the browser's IndexedDB: `store.durable` bound to
//! [`IMPLEMENTATION`], so a run in a page or a Service Worker keeps its state across reloads.
//!
//! `BrowserStore` exists on `wasm32-unknown-unknown` only. `BrowserStore::open` reads every
//! instance of one database into memory; reads are answered from there, and every commit or
//! removal is checked there first, exactly as [`spindle_kernel::MemoryStore`] checks it (the same
//! versions, conflicts and removed versions), then written in one IndexedDB transaction: the
//! instance's record and the events the command emitted, together or not at all. IndexedDB runs
//! one handle's readwrite transactions in the order they were made, so the database follows the
//! order of the commits.
//!
//! The [`Store`](spindle_kernel::Store) trait answers synchronously and IndexedDB does not, so a
//! write lands after the call that made it returns. `BrowserStore::flush` waits for every write
//! made so far; a reopen of the same database after the handle is dropped reads them all. A write
//! that does not land makes every later call fail, since what the handle holds is then ahead of
//! the database. One handle per database: two handles (two tabs) each check versions against
//! their own copy.

/// The implementation name an environment document binds `store.durable` to for this store.
pub const IMPLEMENTATION: &str = "indexeddb";

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod indexeddb;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use indexeddb::BrowserStore;
