//! A [`Store`](spindle_kernel::Store) in the browser's IndexedDB: `store.durable` bound to
//! [`IMPLEMENTATION`], so a run in a page or a Service Worker keeps its state across reloads.
//!
//! `BrowserStore` exists on `wasm32-unknown-unknown` only. `BrowserStore::open` reads every
//! instance of one database into memory; reads are answered from there, and every commit or
//! removal is checked there first, exactly as [`spindle_kernel::MemoryStore`] checks it (the same
//! versions, conflicts and removed versions), then written in one IndexedDB transaction: the
//! instance's record and the events the command emitted, together or not at all; the writes of a
//! set outcome (`Store::commit_all`) are all checked there, then written in one transaction
//! together, every record and every event, or none of them. A change decided on a scan
//! (`Change::scanned`) is refused there as a conflict where the entity's version moved since the
//! scan: every write of the entity this global scope made or heard moves it, and a write another
//! tab made that it has not heard yet does not. IndexedDB runs
//! the readwrite transactions on one database in the order they were made, so the database
//! follows the order of the commits.
//!
//! The [`Store`](spindle_kernel::Store) trait answers synchronously and IndexedDB does not, so a
//! write lands after the call that made it returns. `BrowserStore::flush` waits for every write
//! made so far; a reopen of the same database after the handle is dropped reads them all. A write
//! that does not land makes every later call fail, since what the handle holds is then ahead of
//! the database.
//!
//! Several handles on one database answer as one store. The handles of one global scope (one
//! page, one worker) share what they hold, so each sees the others' writes at once and a change
//! naming a version another moved past is a conflict naming the newer version. Handles in other
//! global scopes (other tabs of the origin) hear each landed write on a `BroadcastChannel` named
//! after the database and take it into what they hold. Until a handle has heard a write it may
//! answer from a stale copy; its own write is then checked inside its IndexedDB transaction
//! against the stored record, does not land when the record moved, and that handle answers
//! nothing more, so no tab overwrites another's write.

/// The implementation name an environment document binds `store.durable` to for this store.
pub const IMPLEMENTATION: &str = "indexeddb";

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod indexeddb;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use indexeddb::BrowserStore;
