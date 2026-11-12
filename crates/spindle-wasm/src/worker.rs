//! The wasm-bindgen entry: [`Spindle`], opened once by a Service Worker and asked to answer each
//! fetch it intercepts.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ess_compiler::EssIr;
use ess_gen::http::Binding;
use js_sys::{Promise, Reflect, Uint8Array};
use serde_json::Value;
use spindle_capability::{Capability, Environment};
use spindle_host::{Answers, Host, Stores, SystemClock};
use spindle_ingress::{HTTP_LOOPBACK, Resolved, Surface};
use spindle_kernel::{Change, Removal, Store, StoreError, Stored};
use spindle_store_browser::BrowserStore;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use web_sys::{Headers, Request, Response, ResponseInit, Url};

use crate::{Config, SERVICE_WORKER};

/// The host a Service Worker answers from: the compiled specification admitted on its
/// environment, its stores opened, and the routes it serves.
#[wasm_bindgen]
pub struct Spindle {
    inner: Rc<Inner>,
}

struct Inner {
    surface: Surface,
    /// The origin of the global scope the host runs in: the only one it answers.
    origin: String,
    base: String,
    host: RefCell<Host>,
    /// Every IndexedDB store the host writes to, flushed before each answer.
    stores: Vec<Kept>,
    /// Settles once the request before the last one is answered.
    tail: RefCell<Promise>,
}

#[wasm_bindgen]
impl Spindle {
    /// Opens the host [`Config`] (as JSON) describes: compiles the specification, admits it on
    /// the environment, opens a `BrowserStore` per component whose state is bound to `indexeddb`,
    /// and lays out the routes.
    ///
    /// # Errors
    /// The configuration is not JSON of that shape; the specification does not compile; the
    /// environment does not parse or does not admit every component; the decisions file or the
    /// binding is refused; `ingress.network` is bound to something other than
    /// [`SERVICE_WORKER`]; a database cannot be opened; or the global scope has no
    /// `location.origin`. The error names which.
    pub async fn open(config: String) -> Result<Spindle, JsError> {
        opened(&config).await.map_err(|why| JsError::new(&why))
    }

    /// Whether this host answers a request for `url` with `method`: `url` is of the origin the
    /// host runs in, and its path, under the configured base, is a route of the specification or
    /// a console route, under any method. The Service Worker shell asks this of every fetch and
    /// leaves every other request to the network.
    pub fn serves(&self, url: &str, method: &str) -> bool {
        let Some((path, _)) = self.inner.target(url) else {
            return false;
        };
        !matches!(
            self.inner.surface.resolve(method, &path),
            Resolved::NotFound
        )
    }

    /// Answers `request`, a `Promise` of its `Response`: routed as the loopback listener routes
    /// it, after every request before it, and once every write it made has landed in IndexedDB.
    pub fn fetch(&self, request: Request) -> Promise {
        let inner = self.inner.clone();
        let mut release = None;
        let mine = Promise::new(&mut |resolve, _| release = Some(resolve));
        let before = inner.tail.replace(mine);
        future_to_promise(async move {
            let _ = JsFuture::from(before).await;
            let answered = inner.answer(request).await;
            if let Some(release) = release {
                let _ = release.call0(&JsValue::UNDEFINED);
            }
            answered.map(JsValue::from)
        })
    }
}

impl Inner {
    /// The path, under the base, and the query of `url`; `None` for a URL of another origin or
    /// outside the base.
    fn target(&self, url: &str) -> Option<(String, String)> {
        let url = Url::new(url).ok()?;
        if url.origin() != self.origin {
            return None;
        }
        let path = url.pathname();
        let under = path.strip_prefix(self.base.trim_end_matches('/'))?;
        let path = if under.starts_with('/') {
            under.to_owned()
        } else if under.is_empty() {
            "/".to_owned()
        } else {
            return None;
        };
        let query = url.search();
        Some((path, query.strip_prefix('?').unwrap_or(&query).to_owned()))
    }

    async fn answer(&self, request: Request) -> Result<Response, JsValue> {
        let Some((path, query)) = self.target(&request.url()) else {
            return respond(&spindle_ingress::Response {
                status: 404,
                allow: None,
                panel: false,
                metrics: false,
                headers: Vec::new(),
                body: serde_json::json!({"refused": format!("{} is outside this host", request.url())})
                    .to_string(),
            });
        };
        let content_type = request.headers().get("content-type")?;
        let body = Uint8Array::new(&JsFuture::from(request.array_buffer()?).await?).to_vec();
        let routed = spindle_ingress::route(
            &self.surface,
            &mut self.host.borrow_mut(),
            spindle_ingress::Request {
                method: request.method(),
                path,
                query,
                content_type,
                body,
            },
        );
        for store in &self.stores {
            // The store is taken out of its cell while the flush awaits, so no borrow is held
            // across it; requests are answered one at a time, so the host does not reach for it
            // in between.
            let Some(flushing) = store.take() else {
                continue;
            };
            let flushed = flushing.flush().await;
            store.replace(Some(flushing));
            flushed.map_err(|error| JsError::new(&error.to_string()))?;
        }
        respond(&routed.response)
    }
}

/// The statuses the Fetch standard gives a null body: a `Response` built with one and a body is
/// refused (`TypeError`), so the fetch it answers would reject.
const NULL_BODY: [u16; 5] = [101, 103, 204, 205, 304];

/// `response` as a `Response`: its status, its body as [`spindle_ingress::CONTENT_TYPE`] (or the
/// panel's or the metrics' media type, where the response says it is one of them) where
/// it has one and the status may carry one ([`NULL_BODY`]), `Allow` where it carries it, and the
/// headers the binding declares for it.
fn respond(response: &spindle_ingress::Response) -> Result<Response, JsValue> {
    let headers = Headers::new()?;
    let body = (!response.body.is_empty() && !NULL_BODY.contains(&response.status))
        .then_some(response.body.as_str());
    if response.panel {
        headers.set("content-type", spindle_ingress::PANEL_MEDIA)?;
        headers.set("content-security-policy", spindle_ingress::PANEL_POLICY)?;
    } else if response.metrics && body.is_some() {
        headers.set("content-type", spindle_ingress::METRICS_MEDIA)?;
    } else if body.is_some() {
        headers.set("content-type", spindle_ingress::CONTENT_TYPE)?;
    }
    if let Some(allow) = &response.allow {
        headers.set("allow", allow)?;
    }
    // The headers the binding declares, after those written above, none of which one of them
    // replaces.
    for (name, value) in &response.headers {
        if !headers.has(name)? {
            headers.append(name, &byte_string(value))?;
        }
    }
    let init = ResponseInit::new();
    init.set_status(response.status);
    init.set_headers(&headers);
    Response::new_with_opt_str_and_init(body, &init)
}

/// `value`'s UTF-8 bytes as a ByteString, one code point per byte, so a declared header carries the
/// bytes the native listener and ESS write: `Headers` takes a header value as a ByteString, and
/// throws on a code point above U+00FF or writes one up to it as a single byte.
fn byte_string(value: &str) -> String {
    value.bytes().map(char::from).collect()
}

async fn opened(config: &str) -> Result<Spindle, String> {
    let config: Config =
        serde_json::from_str(config).map_err(|error| format!("the configuration: {error}"))?;
    let (names, texts): (Vec<String>, Vec<String>) = config.sources.into_iter().unzip();
    let ir = spindle_spec::compile(&names, &texts)
        .map_err(|error| format!("the specification: {error}"))?;
    let environment = Environment::parse(&config.environment)
        .map_err(|error| format!("the environment: {error}"))?;
    let placements =
        spindle_host::admit(&ir, &environment).map_err(|refused| refused.to_string())?;

    let mut stores = Stores::new();
    let mut browser = Vec::new();
    let mut reached = Vec::new();
    for (component, bindings) in &placements {
        let bound = |capability: &str| {
            bindings
                .iter()
                .find(|binding| binding.capability.as_str() == capability)
                .map(|binding| binding.implementation.as_str())
        };
        let state = bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral"));
        if state == Some(spindle_store_browser::IMPLEMENTATION) {
            let name = format!("{}/{component}", config.database);
            let store = Rc::new(RefCell::new(Some(
                BrowserStore::open(&name)
                    .await
                    .map_err(|error| format!("opening {name}: {error}"))?,
            )));
            browser.push(store.clone());
            stores.insert(component.clone(), Box::new(Shared(store)));
        }
        match bound(Capability::INGRESS_NETWORK) {
            None => {}
            Some(SERVICE_WORKER) => reached.push(component.clone()),
            Some(other) => {
                return Err(format!(
                    "{component} has ingress.network bound to `{other}`; a Service Worker serves \
                     `{SERVICE_WORKER}` only"
                ));
            }
        }
    }

    let decider = config
        .decisions
        .map(|text| Answers::parse(&ir, &text).map(Answers::served))
        .transpose()
        .map_err(|error| format!("the decisions file: {error}"))?;
    let binding = config.binding.map(|text| binding(&ir, &text)).transpose()?;
    let surface = Surface::new(&ir, &reached, binding.as_ref())
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .map_err(|refused| format!("the routes: {refused}"))?;
    let host = Host::supplied(ir, &environment, Arc::new(SystemClock), decider, stores)
        .map_err(|error| error.to_string())?;
    let origin = origin().ok_or("the global scope has no location.origin")?;
    Ok(Spindle {
        inner: Rc::new(Inner {
            surface,
            origin,
            base: config.base,
            host: RefCell::new(host),
            stores: browser,
            tail: RefCell::new(Promise::resolve(&JsValue::UNDEFINED)),
        }),
    })
}

/// The origin of the global scope (`self.location.origin`, a Service Worker's or a page's).
fn origin() -> Option<String> {
    let location = Reflect::get(&js_sys::global(), &"location".into()).ok()?;
    Reflect::get(&location, &"origin".into()).ok()?.as_string()
}

/// An `ess-http/1` binding's text, YAML or JSON, compiled by ESS against `ir`.
fn binding(ir: &EssIr, text: &str) -> Result<Binding, String> {
    let spec = if serde_json::from_str::<Value>(text).is_ok() {
        ess_http::HttpSpec::from_json(text).map_err(|error| error.to_string())
    } else {
        ess_http::HttpSpec::from_yaml(text).map_err(|error| error.to_string())
    }
    .map_err(|error| format!("the binding: {error}"))?;
    let compiled = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .map_err(|diagnostics| format!("the binding was refused:\n{diagnostics}"))?;
    Binding::new(compiled).map_err(|open| format!("the binding cannot be served:\n{open}"))
}

/// A `BrowserStore` both the host and this crate hold: `None` only while a flush awaits.
type Kept = Rc<RefCell<Option<BrowserStore>>>;

/// A `BrowserStore` the host writes through while this crate keeps a handle to flush it.
struct Shared(Kept);

impl Shared {
    fn with<T>(
        &self,
        act: impl FnOnce(&mut BrowserStore) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        match self.0.borrow_mut().as_mut() {
            Some(store) => act(store),
            None => Err(StoreError::Backend(
                "the IndexedDB store is being flushed".to_owned(),
            )),
        }
    }
}

impl Store for Shared {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.with(|store| store.load(entity, identity))
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.with(|store| store.commit(change))
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.with(|store| store.remove(removal))
    }

    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        self.with(|store| store.commit_all(writes))
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.with(|store| store.scan(entity))
    }

    fn attribute(&mut self, origin: Option<spindle_kernel::Origin>) {
        if let Some(store) = self.0.borrow_mut().as_mut() {
            store.attribute(origin);
        }
    }

    fn commit_events(
        &mut self,
        events: &[spindle_kernel::Event],
        scanned: &[spindle_kernel::Scanned],
    ) -> Result<(), StoreError> {
        self.with(|store| store.commit_events(events, scanned))
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.with(|store| store.forget_events())
    }

    fn events_after(
        &mut self,
        position: u64,
        limit: usize,
    ) -> Result<Vec<spindle_kernel::Committed>, StoreError> {
        self.with(|store| store.events_after(position, limit))
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.with(|store| store.events_head())
    }
}
