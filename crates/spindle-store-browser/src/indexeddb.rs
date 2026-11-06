//! The store on IndexedDB. See the crate documentation for how it holds and writes instances.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use js_sys::{Array, Promise, Reflect};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use spindle_kernel::{Change, Event, Removal, Row, Store, StoreError, Stored};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    DomException, IdbDatabase, IdbFactory, IdbObjectStoreParameters, IdbOpenDbRequest, IdbRequest,
    IdbTransaction, IdbTransactionMode,
};

/// The schema version of the database: its two object stores.
const SCHEMA: u32 = 1;
/// The object store of instances: one record per instance ever written, keyed by
/// `[entity, identity text]`.
const ROWS: &str = "rows";
/// The object store of events, in commit order under an increasing key.
const EVENTS: &str = "events";

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

/// An instance's key: its entity and its identity as canonical JSON text.
type Key = (String, String);

/// Instances in one IndexedDB database of the browser's origin: `store.durable` bound to
/// `indexeddb`.
pub struct BrowserStore {
    name: String,
    database: IdbDatabase,
    rows: BTreeMap<Key, Stored>,
    /// The last version of each removed instance, as [`spindle_kernel::MemoryStore`] keeps it.
    removed: BTreeMap<Key, u64>,
    /// Settles when the last write's transaction completes or aborts.
    written: Option<Promise>,
    /// Why a write did not land, once one did not.
    failed: Rc<RefCell<Option<String>>>,
}

impl std::fmt::Debug for BrowserStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserStore")
            .field("name", &self.name)
            .field("instances", &self.rows.len())
            .finish_non_exhaustive()
    }
}

impl BrowserStore {
    /// Opens the database `name` of this origin, creating it when absent, and reads every
    /// instance it holds.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when this global scope has no IndexedDB, or the database cannot be
    /// opened or read.
    pub async fn open(name: &str) -> Result<Self, StoreError> {
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

        let mut store = Self {
            name: name.to_owned(),
            database,
            rows: BTreeMap::new(),
            removed: BTreeMap::new(),
            written: None,
            failed: Rc::new(RefCell::new(None)),
        };
        for record in store.read_all(ROWS).await? {
            let record: Record = parse(&record, "an instance")?;
            let key = (record.entity, record.identity);
            match record.row {
                Some(row) => {
                    store.rows.insert(
                        key,
                        Stored {
                            row,
                            version: record.version,
                        },
                    );
                }
                None => {
                    store.removed.insert(key, record.version);
                }
            }
        }
        Ok(store)
    }

    /// Waits until every write made so far has landed in the database.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when a write did not land.
    pub async fn flush(&self) -> Result<(), StoreError> {
        if let Some(written) = &self.written {
            JsFuture::from(written.clone()).await.map_err(js)?;
        }
        self.healthy()
    }

    /// Every event recorded in the database, in commit order, the writes made so far included.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the database cannot be read or a write did not land.
    pub async fn events(&self) -> Result<Vec<Event>, StoreError> {
        self.healthy()?;
        self.read_all(EVENTS)
            .await?
            .iter()
            .map(|text| {
                parse::<Recorded>(text, "an event").map(|recorded| Event {
                    name: recorded.event,
                    payload: recorded.payload,
                })
            })
            .collect()
    }

    /// Every value of the object store `name`, each a JSON text, in key order.
    async fn read_all(&self, name: &str) -> Result<Vec<String>, StoreError> {
        let transaction = self.database.transaction_with_str(name).map_err(js)?;
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

    /// Writes `record` and `events` in one readwrite transaction. The requests are queued before
    /// this returns; the transaction commits once control returns to the browser, after every
    /// transaction queued before it.
    fn write(
        &mut self,
        record: &Record,
        identity: &Value,
        events: &[Event],
    ) -> Result<(), StoreError> {
        let record_text = serde_json::to_string(record).map_err(json)?;
        let event_texts = events
            .iter()
            .map(|event| {
                serde_json::to_string(&Recorded {
                    entity: record.entity.clone(),
                    identity: identity.clone(),
                    event: event.name.clone(),
                    payload: event.payload.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(json)?;
        let scope = Array::of2(&ROWS.into(), &EVENTS.into());
        let transaction = self
            .database
            .transaction_with_str_sequence_and_mode(&scope, IdbTransactionMode::Readwrite)
            .map_err(js)?;
        let queued = (|| -> Result<(), JsValue> {
            let key = Array::of2(
                &record.entity.as_str().into(),
                &record.identity.as_str().into(),
            );
            transaction
                .object_store(ROWS)?
                .put_with_key(&record_text.into(), &key)?;
            let log = transaction.object_store(EVENTS)?;
            for text in event_texts {
                log.add(&text.into())?;
            }
            Ok(())
        })();
        if let Err(error) = queued {
            let _ = transaction.abort();
            return Err(js(error));
        }
        self.written = Some(watch(&transaction, &self.failed));
        Ok(())
    }
}

impl Drop for BrowserStore {
    /// Closes the connection; the transactions already queued still complete.
    fn drop(&mut self) {
        self.database.close();
    }
}

impl Store for BrowserStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.healthy()?;
        Ok(self.rows.get(&key(entity, identity)).cloned())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.healthy()?;
        let key = key(&change.entity, &change.identity);
        let actual = self.rows.get(&key).map(|stored| stored.version);
        if actual != change.expected {
            return Err(StoreError::Conflict {
                entity: change.entity.clone(),
                identity: key.1,
                expected: change.expected,
                actual,
            });
        }
        let version = actual
            .or_else(|| self.removed.get(&key).copied())
            .unwrap_or(0)
            + 1;
        let record = Record {
            entity: key.0.clone(),
            identity: key.1.clone(),
            version,
            row: Some(change.row.clone()),
        };
        self.write(&record, &change.identity, &change.events)?;
        self.rows.insert(
            key,
            Stored {
                row: change.row.clone(),
                version,
            },
        );
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.healthy()?;
        let key = key(&removal.entity, &removal.identity);
        let actual = self.rows.get(&key).map(|stored| stored.version);
        if actual != Some(removal.expected) {
            return Err(StoreError::Conflict {
                entity: removal.entity.clone(),
                identity: key.1,
                expected: Some(removal.expected),
                actual,
            });
        }
        let record = Record {
            entity: key.0.clone(),
            identity: key.1.clone(),
            version: removal.expected,
            row: None,
        };
        self.write(&record, &removal.identity, &removal.events)?;
        self.rows.remove(&key);
        self.removed.insert(key, removal.expected);
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError> {
        self.healthy()?;
        self.rows
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
/// it aborted.
fn watch(transaction: &IdbTransaction, failed: &Rc<RefCell<Option<String>>>) -> Promise {
    Promise::new(&mut |resolve, _| {
        let failed = failed.clone();
        let aborted = transaction.clone();
        let done = Closure::once_into_js(move |event: web_sys::Event| {
            if event.type_() != "complete" {
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
