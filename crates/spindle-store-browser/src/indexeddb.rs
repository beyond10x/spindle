//! The store on IndexedDB. See the crate documentation for how it holds and writes instances.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use js_sys::{Array, Promise, Reflect};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use spindle_kernel::{Change, Event, Removal, Row, Store, StoreError, Stored, Write};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    BroadcastChannel, DomException, IdbDatabase, IdbFactory, IdbObjectStoreParameters,
    IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbTransactionMode, MessageEvent,
};

/// The schema version of the database: its two object stores.
const SCHEMA: u32 = 1;
/// The object store of instances: one record per instance ever written, keyed by
/// `[entity, identity text]`.
const ROWS: &str = "rows";
/// The object store of events, in commit order under an increasing key.
const EVENTS: &str = "events";
/// The prefix of the broadcast channel a database's handles announce their landed writes on.
const CHANNEL: &str = "spindle-store-browser:";

/// An instance as the `rows` object store holds it: its row, or, once removed, only the version
/// it was removed at, so an instance created again under its identity goes on from it.
#[derive(Serialize, Deserialize)]
struct Record {
    entity: String,
    identity: String,
    version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    row: Option<Row>,
}

/// An event as the `events` object store holds it.
#[derive(Serialize, Deserialize)]
struct Recorded {
    entity: String,
    identity: Value,
    event: String,
    payload: Map<String, Value>,
}

/// One instance [`Shared::write`] writes: the record it is written as, where this global scope
/// believed it stood before the writes, and the events recorded with it.
struct Pending<'w> {
    record: Record,
    believed: Standing,
    identity: &'w Value,
    events: &'w [Event],
}

/// One instance of a transaction [`Shared::write`] queued: its key, what is put, and the read of
/// the record it found.
struct Entry {
    key: Array,
    record_text: String,
    event_texts: Vec<String>,
    believed: Standing,
    instance: String,
    read: IdbRequest,
}

impl Entry {
    /// Refuses the write where the database holds the instance elsewhere than this global scope
    /// believed.
    fn check(&self) -> Result<(), String> {
        let stored = self.read.result().map_err(|error| js(error).to_string())?;
        let found = if stored.is_undefined() {
            None
        } else {
            let text = stored
                .as_string()
                .ok_or_else(|| format!("the record of {} is not text", self.instance))?;
            let found = parse::<Record>(&text, "an instance").map_err(|e| e.to_string())?;
            Some((found.version, found.row.is_none()))
        };
        if found != self.believed {
            return Err(format!(
                "another handle wrote {} first: the database holds {}, this handle held {}",
                self.instance,
                describe_standing(found),
                describe_standing(self.believed)
            ));
        }
        Ok(())
    }
}

/// An instance's key: its entity and its identity as canonical JSON text.
type Key = (String, String);

/// Where an instance stands: its version and whether it is removed, or `None` when it was never
/// written. A later standing orders after an earlier one: versions only grow, and a removal keeps
/// the version it removed.
type Standing = Option<(u64, bool)>;

thread_local! {
    /// The database each open handle of this global scope holds, by name: a second `open` of a
    /// name shares it.
    static OPEN: RefCell<BTreeMap<String, Weak<RefCell<Shared>>>> =
        const { RefCell::new(BTreeMap::new()) };
}

/// Instances in one IndexedDB database of the browser's origin: `store.durable` bound to
/// `indexeddb`.
pub struct BrowserStore {
    shared: Rc<RefCell<Shared>>,
}

/// What every handle of one global scope on one database holds together.
struct Shared {
    name: String,
    database: IdbDatabase,
    rows: BTreeMap<Key, Stored>,
    /// The last version of each removed instance, as [`spindle_kernel::MemoryStore`] keeps it.
    removed: BTreeMap<Key, u64>,
    /// Settles when the last write's transaction completes or aborts.
    written: Option<Promise>,
    /// Why a write did not land, once one did not.
    failed: Rc<RefCell<Option<String>>>,
    /// The channel landed writes are announced on, and heard from other global scopes, where this
    /// global scope has one.
    channel: Option<BroadcastChannel>,
    listener: Option<Closure<dyn FnMut(MessageEvent)>>,
}

impl std::fmt::Debug for BrowserStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shared = self.shared.borrow();
        formatter
            .debug_struct("BrowserStore")
            .field("name", &shared.name)
            .field("instances", &shared.rows.len())
            .finish_non_exhaustive()
    }
}

impl BrowserStore {
    /// Opens the database `name` of this origin, creating it when absent, and reads every
    /// instance it holds. A handle of this global scope already open on `name` is shared: the
    /// new handle answers as that one does.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when this global scope has no IndexedDB, or the database cannot be
    /// opened or read.
    pub async fn open(name: &str) -> Result<Self, StoreError> {
        if let Some(shared) = held(name) {
            return Ok(Self { shared });
        }
        let opened = Shared::open(name).await?;
        if let Some(shared) = held(name) {
            return Ok(Self { shared });
        }
        OPEN.with(|open| {
            open.borrow_mut()
                .insert(name.to_owned(), Rc::downgrade(&opened));
        });
        Ok(Self { shared: opened })
    }

    /// Opens the database `name` as [`BrowserStore::open`] does, sharing nothing with the other
    /// handles of this global scope, as a handle in another tab shares nothing with them.
    ///
    /// # Errors
    /// As [`BrowserStore::open`].
    #[doc(hidden)]
    pub async fn open_apart(name: &str) -> Result<Self, StoreError> {
        Ok(Self {
            shared: Shared::open(name).await?,
        })
    }

    /// Waits until every write made so far has landed in the database.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when a write did not land.
    pub async fn flush(&self) -> Result<(), StoreError> {
        let written = self.shared.borrow().written.clone();
        if let Some(written) = written {
            JsFuture::from(written).await.map_err(js)?;
        }
        self.shared.borrow().healthy()
    }

    /// Every event recorded in the database, in commit order, the writes made so far included.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the database cannot be read or a write did not land.
    pub async fn events(&self) -> Result<Vec<Event>, StoreError> {
        let database = {
            let shared = self.shared.borrow();
            shared.healthy()?;
            shared.database.clone()
        };
        let texts = read_all(&database, EVENTS).await?;
        self.shared.borrow().healthy()?;
        texts
            .iter()
            .map(|text| {
                parse::<Recorded>(text, "an event").map(|recorded| Event {
                    name: recorded.event,
                    payload: recorded.payload,
                })
            })
            .collect()
    }
}

/// The database a handle of this global scope holds open under `name`.
fn held(name: &str) -> Option<Rc<RefCell<Shared>>> {
    OPEN.with(|open| {
        open.borrow()
            .get(name)
            .and_then(Weak::upgrade)
            .filter(|shared| shared.borrow().healthy().is_ok())
    })
}

impl Shared {
    /// Opens the database `name`, listens for the writes other global scopes announce, then
    /// reads every instance it holds.
    async fn open(name: &str) -> Result<Rc<RefCell<Self>>, StoreError> {
        let request = factory()?.open_with_u32(name, SCHEMA).map_err(js)?;
        let target = request.clone();
        let upgrade = Closure::<dyn FnMut(web_sys::Event) -> Result<(), JsValue>>::new(
            move |_: web_sys::Event| provision(&target),
        );
        request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
        let opened = settled(&request).await;
        request.set_onupgradeneeded(None);
        drop(upgrade);
        let database = opened?
            .dyn_into::<IdbDatabase>()
            .map_err(|_| StoreError::Backend(format!("{name} did not open as a database")))?;

        let shared = Rc::new(RefCell::new(Self {
            name: name.to_owned(),
            database: database.clone(),
            rows: BTreeMap::new(),
            removed: BTreeMap::new(),
            written: None,
            failed: Rc::new(RefCell::new(None)),
            channel: None,
            listener: None,
        }));
        if let Some(channel) = channel(name) {
            let hearer = Rc::downgrade(&shared);
            let listener = Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
                let Some(text) = message.data().as_string() else {
                    return;
                };
                let Ok(record) = serde_json::from_str::<Record>(&text) else {
                    return;
                };
                if let Some(shared) = hearer.upgrade()
                    && let Ok(mut shared) = shared.try_borrow_mut()
                {
                    shared.learn(record);
                }
            });
            channel.set_onmessage(Some(listener.as_ref().unchecked_ref()));
            let mut held = shared.borrow_mut();
            held.channel = Some(channel);
            held.listener = Some(listener);
        }
        for text in read_all(&database, ROWS).await? {
            shared
                .borrow_mut()
                .learn(parse::<Record>(&text, "an instance")?);
        }
        Ok(shared)
    }

    /// Takes `record` as where its instance stands, unless what is held stands later.
    fn learn(&mut self, record: Record) {
        let key = (record.entity, record.identity);
        let heard = Some((record.version, record.row.is_none()));
        if heard <= self.standing(&key) {
            return;
        }
        match record.row {
            Some(row) => {
                self.removed.remove(&key);
                self.rows.insert(
                    key,
                    Stored {
                        row,
                        version: record.version,
                    },
                );
            }
            None => {
                self.rows.remove(&key);
                self.removed.insert(key, record.version);
            }
        }
    }

    /// Where the instance under `key` stands in what this global scope holds.
    fn standing(&self, key: &Key) -> Standing {
        match self.rows.get(key) {
            Some(stored) => Some((stored.version, false)),
            None => self.removed.get(key).map(|version| (*version, true)),
        }
    }

    /// Refuses every read and write once a write has not landed: what the store holds in memory
    /// is then ahead of the database.
    fn healthy(&self) -> Result<(), StoreError> {
        match &*self.failed.borrow() {
            Some(why) => Err(StoreError::Backend(format!(
                "a write to {} did not land ({why}); the store answers nothing more",
                self.name
            ))),
            None => Ok(()),
        }
    }

    /// Checks `write` against what this global scope holds, as [`spindle_kernel::MemoryStore`]
    /// checks it, and takes it in: the record the instance is then written as.
    fn stage(&mut self, write: &Write) -> Result<Record, StoreError> {
        let (entity, identity) = match write {
            Write::Commit(change) => (&change.entity, &change.identity),
            Write::Remove(removal) => (&removal.entity, &removal.identity),
        };
        let key = key(entity, identity);
        let actual = self.rows.get(&key).map(|stored| stored.version);
        let named = match write {
            Write::Commit(change) => change.expected,
            Write::Remove(removal) => Some(removal.expected),
        };
        if actual != named {
            return Err(StoreError::Conflict {
                entity: entity.clone(),
                identity: key.1,
                expected: named,
                actual,
            });
        }
        let (version, row) = match write {
            Write::Commit(change) => {
                let version = self.standing(&key).map_or(0, |(version, _)| version) + 1;
                self.removed.remove(&key);
                self.rows.insert(
                    key.clone(),
                    Stored {
                        row: change.row.clone(),
                        version,
                    },
                );
                (version, Some(change.row.clone()))
            }
            Write::Remove(removal) => {
                self.rows.remove(&key);
                self.removed.insert(key.clone(), removal.expected);
                (removal.expected, None)
            }
        };
        Ok(Record {
            entity: key.0,
            identity: key.1,
            version,
            row,
        })
    }

    /// Writes every record of `pending` and its events in one readwrite transaction, if the
    /// database still holds each instance where this global scope believed it stood before them;
    /// otherwise the transaction aborts and none of them lands. The requests are queued before
    /// this returns; the transaction runs once control returns to the browser, after every
    /// transaction on the database queued before it, whichever global scope queued it.
    fn write(&mut self, pending: &[Pending<'_>]) -> Result<(), StoreError> {
        let scope = Array::of2(&ROWS.into(), &EVENTS.into());
        let transaction = self
            .database
            .transaction_with_str_sequence_and_mode(&scope, IdbTransactionMode::Readwrite)
            .map_err(js)?;
        let mut entries = Vec::with_capacity(pending.len());
        let mut announced = Vec::with_capacity(pending.len());
        for Pending {
            record,
            believed,
            identity,
            events,
        } in pending
        {
            let record_text = serde_json::to_string(record).map_err(json)?;
            let event_texts = events
                .iter()
                .map(|event| {
                    serde_json::to_string(&Recorded {
                        entity: record.entity.clone(),
                        identity: (*identity).clone(),
                        event: event.name.clone(),
                        payload: event.payload.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(json)?;
            let key = Array::of2(
                &record.entity.as_str().into(),
                &record.identity.as_str().into(),
            );
            // Every read is queued before any put, so each reads the record as it stood before
            // this transaction.
            let request = transaction
                .object_store(ROWS)
                .and_then(|rows| rows.get(&key))
                .map_err(|error| {
                    let _ = transaction.abort();
                    js(error)
                })?;
            announced.push(record_text.clone());
            entries.push(Entry {
                key,
                record_text,
                event_texts,
                believed: *believed,
                instance: format!("{} {}", record.entity, record.identity),
                read: request,
            });
        }
        let Some(last) = entries.last().map(|entry| entry.read.clone()) else {
            return Ok(());
        };
        let failed = self.failed.clone();
        let writing = transaction.clone();
        // Requests of one transaction succeed in the order they were made: once the last read
        // has, every read has.
        let check = Closure::once_into_js(move |event: web_sys::Event| {
            if event.type_() != "success" {
                return;
            }
            let written = (|| -> Result<(), String> {
                if failed.borrow().is_some() {
                    return Err("an earlier write did not land".to_owned());
                }
                for entry in &entries {
                    entry.check()?;
                }
                let put = || -> Result<(), JsValue> {
                    let rows = writing.object_store(ROWS)?;
                    let log = writing.object_store(EVENTS)?;
                    for entry in entries {
                        rows.put_with_key(&entry.record_text.into(), &entry.key)?;
                        for text in entry.event_texts {
                            log.add(&text.into())?;
                        }
                    }
                    Ok(())
                };
                put().map_err(|error| js(error).to_string())
            })();
            if let Err(why) = written {
                failed.borrow_mut().get_or_insert(why);
                let _ = writing.abort();
            }
        });
        last.set_onsuccess(Some(check.unchecked_ref()));
        last.set_onerror(Some(check.unchecked_ref()));
        let announce = self.channel.clone().map(|channel| (channel, announced));
        self.written = Some(watch(&transaction, &self.failed, announce));
        Ok(())
    }
}

impl Drop for Shared {
    /// Stops listening and closes the connection; the transactions already queued still complete
    /// and are still announced.
    fn drop(&mut self) {
        if let Some(channel) = &self.channel {
            channel.set_onmessage(None);
        }
        self.database.close();
        let _ = OPEN.try_with(|open| {
            if let Ok(mut open) = open.try_borrow_mut() {
                open.retain(|_, shared| shared.strong_count() > 0);
            }
        });
    }
}

impl Store for BrowserStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        let shared = self.shared.borrow();
        shared.healthy()?;
        Ok(shared.rows.get(&key(entity, identity)).cloned())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.commit_all(&[Write::Commit(change.clone())])
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.commit_all(&[Write::Remove(removal.clone())])
    }

    /// Checks and takes in each write in turn, then writes them all in one IndexedDB
    /// transaction; where one is refused, or the transaction cannot be queued, every instance is
    /// put back as this global scope held it.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        let mut shared = self.shared.borrow_mut();
        shared.healthy()?;
        if writes.is_empty() {
            return Ok(());
        }
        let mut before: BTreeMap<Key, (Option<Stored>, Option<u64>)> = BTreeMap::new();
        let staged = (|| {
            let mut pending = Vec::with_capacity(writes.len());
            for write in writes {
                let (entity, identity, events) = match write {
                    Write::Commit(change) => (&change.entity, &change.identity, &change.events),
                    Write::Remove(removal) => (&removal.entity, &removal.identity, &removal.events),
                };
                let key = key(entity, identity);
                let held = before.entry(key.clone()).or_insert_with(|| {
                    (
                        shared.rows.get(&key).cloned(),
                        shared.removed.get(&key).copied(),
                    )
                });
                let believed = match held {
                    (Some(stored), _) => Some((stored.version, false)),
                    (None, removed) => removed.map(|version| (version, true)),
                };
                pending.push(Pending {
                    record: shared.stage(write)?,
                    believed,
                    identity,
                    events,
                });
            }
            shared.write(&pending)
        })();
        if staged.is_err() {
            for (key, (row, removed)) in before {
                match row {
                    Some(row) => shared.rows.insert(key.clone(), row),
                    None => shared.rows.remove(&key),
                };
                match removed {
                    Some(version) => shared.removed.insert(key, version),
                    None => shared.removed.remove(&key),
                };
            }
        }
        staged
    }

    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError> {
        let shared = self.shared.borrow();
        shared.healthy()?;
        shared
            .rows
            .iter()
            .filter(|((held, _), _)| held == entity)
            .map(|((_, identity), stored)| {
                Ok((
                    parse::<Value>(identity, "a stored identity")?,
                    stored.clone(),
                ))
            })
            .collect()
    }
}

fn key(entity: &str, identity: &Value) -> Key {
    (entity.to_owned(), identity.to_string())
}

fn describe_standing(standing: Standing) -> String {
    match standing {
        None => "no record".to_owned(),
        Some((version, false)) => format!("version {version}"),
        Some((version, true)) => format!("a removal at version {version}"),
    }
}

/// Every value of the object store `name`, each a JSON text, in key order.
async fn read_all(database: &IdbDatabase, name: &str) -> Result<Vec<String>, StoreError> {
    let transaction = database.transaction_with_str(name).map_err(js)?;
    let request = transaction
        .object_store(name)
        .map_err(js)?
        .get_all()
        .map_err(js)?;
    Array::from(&settled(&request).await?)
        .iter()
        .map(|value| {
            value
                .as_string()
                .ok_or_else(|| StoreError::Backend(format!("a value in {name} is not text")))
        })
        .collect()
}

/// The broadcast channel of the database `name`, where this global scope has broadcast
/// channels.
fn channel(name: &str) -> Option<BroadcastChannel> {
    let constructor =
        Reflect::get(&js_sys::global(), &JsValue::from_str("BroadcastChannel")).ok()?;
    if constructor.is_undefined() || constructor.is_null() {
        return None;
    }
    BroadcastChannel::new(&format!("{CHANNEL}{name}")).ok()
}

/// The IndexedDB factory of this global scope: a window's, or a worker's.
fn factory() -> Result<IdbFactory, StoreError> {
    let value = Reflect::get(&js_sys::global(), &JsValue::from_str("indexedDB")).map_err(js)?;
    if value.is_undefined() || value.is_null() {
        return Err(StoreError::Backend(
            "this global scope has no IndexedDB".to_owned(),
        ));
    }
    value
        .dyn_into::<IdbFactory>()
        .map_err(|_| StoreError::Backend("indexedDB is not an IDBFactory".to_owned()))
}

/// Creates the object stores a database opened at [`SCHEMA`] for the first time lacks.
fn provision(request: &IdbOpenDbRequest) -> Result<(), JsValue> {
    let database = request.result()?.dyn_into::<IdbDatabase>()?;
    let present = database.object_store_names();
    if !present.contains(ROWS) {
        database.create_object_store(ROWS)?;
    }
    if !present.contains(EVENTS) {
        let parameters = IdbObjectStoreParameters::new();
        parameters.set_auto_increment(true);
        database.create_object_store_with_optional_parameters(EVENTS, &parameters)?;
    }
    Ok(())
}

/// The result of `request` once it succeeds, or why it failed.
async fn settled(request: &IdbRequest) -> Result<JsValue, StoreError> {
    let promise = Promise::new(&mut |resolve, _| {
        let done = Closure::once_into_js(move |event: web_sys::Event| {
            let _ = resolve.call1(&JsValue::UNDEFINED, &JsValue::from_str(&event.type_()));
        });
        request.set_onsuccess(Some(done.unchecked_ref()));
        request.set_onerror(Some(done.unchecked_ref()));
    });
    let kind = JsFuture::from(promise).await.map_err(js)?;
    if kind.as_string().as_deref() == Some("success") {
        return request.result().map_err(js);
    }
    Err(StoreError::Backend(match request.error() {
        Ok(Some(error)) => describe(&error),
        _ => "an IndexedDB request failed".to_owned(),
    }))
}

/// A promise that settles when `transaction` completes or aborts, recording into `failed` why
/// it aborted, and once it completes posting each of `announce`'s records on its channel.
fn watch(
    transaction: &IdbTransaction,
    failed: &Rc<RefCell<Option<String>>>,
    announce: Option<(BroadcastChannel, Vec<String>)>,
) -> Promise {
    Promise::new(&mut |resolve, _| {
        let failed = failed.clone();
        let aborted = transaction.clone();
        let announce = announce.clone();
        let done = Closure::once_into_js(move |event: web_sys::Event| {
            if event.type_() == "complete" {
                if let Some((channel, records)) = announce {
                    for record in records {
                        let _ = channel.post_message(&JsValue::from_str(&record));
                    }
                }
            } else {
                let why = aborted
                    .error()
                    .map(|error| describe(&error))
                    .unwrap_or_else(|| "the transaction was aborted".to_owned());
                failed.borrow_mut().get_or_insert(why);
            }
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        transaction.set_oncomplete(Some(done.unchecked_ref()));
        transaction.set_onabort(Some(done.unchecked_ref()));
    })
}

fn parse<T: for<'de> Deserialize<'de>>(text: &str, what: &str) -> Result<T, StoreError> {
    serde_json::from_str(text).map_err(|error| StoreError::Backend(format!("{what}: {error}")))
}

fn json(error: serde_json::Error) -> StoreError {
    StoreError::Backend(error.to_string())
}

fn describe(error: &DomException) -> String {
    format!("{}: {}", error.name(), error.message())
}

fn js(error: JsValue) -> StoreError {
    StoreError::Backend(match error.dyn_ref::<DomException>() {
        Some(exception) => describe(exception),
        None => error.as_string().unwrap_or_else(|| format!("{error:?}")),
    })
}
