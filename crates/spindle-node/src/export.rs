//! The wasm-bindgen entry: [`Spindle`], created by a Node program and handed each request.

use std::cell::RefCell;
use std::sync::Arc;

use js_sys::{Array, Object, Promise, Reflect, Uint8Array};
use spindle_embed::{Opened, Opening};
use spindle_host::{Stores, SystemClock};
use spindle_ingress::{Request, Surface};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::{Config, NODE};

/// What every call on a disposed instance throws.
const DISPOSED: &str = "this Spindle was disposed";

/// A host a Node program serves a specification from.
#[wasm_bindgen]
pub struct Spindle {
    /// `None` once disposed.
    opened: RefCell<Option<Opened>>,
    /// The not-served report, as `run` prints it before listening.
    report: Vec<String>,
}

#[wasm_bindgen]
impl Spindle {
    /// Opens the host `config` ([`Config`], a JSON string or a plain object) describes: compiles
    /// the specification, admits it on the environment, keeps every store in memory, and lays out
    /// the routes with the console routes.
    ///
    /// # Errors
    /// An `Error` naming the reason: the configuration is not of [`Config`]'s shape; the
    /// specification does not compile; the environment does not parse or does not admit every
    /// component; a component's state is bound to anything but `memory`, or its `ingress.network`
    /// to anything but [`NODE`]; the decisions file or the binding is refused; the host refuses
    /// what admission bound (an external outcome nothing answers among them); or the routes serve
    /// no operation, the report's lines then the reason `run` gives.
    pub fn create(config: JsValue) -> Result<Spindle, JsError> {
        created(&config).map_err(|why| JsError::new(&why))
    }

    /// The not-served report: each operation not served and each outcome answered `501` where a
    /// request selects it, with why, one line each, as `run` prints them before listening.
    ///
    /// # Errors
    /// The instance was disposed.
    #[wasm_bindgen(js_name = notServed)]
    pub fn not_served(&self) -> Result<Vec<String>, JsError> {
        if self.opened.borrow().is_none() {
            return Err(JsError::new(DISPOSED));
        }
        Ok(self.report.clone())
    }

    /// Answers `request` (`{ method, path, query?, headers?, body? }`) with a `Promise` of
    /// `{ status, headers, body }`: routed as the loopback listener routes it, after every request
    /// handed over before it. `path` may carry its query after `?`; `query`, without the `?`,
    /// wins where given. `headers` is a plain object or an array of `[name, value]`; only
    /// `content-type` is read. `body` is a string or a `Uint8Array`. The answer's `headers` is an
    /// array of `[name, value]` and its `body` a `Uint8Array`, empty where the status takes none.
    /// The promise rejects with an `Error` for a request without a method or a path.
    ///
    /// # Errors
    /// The instance was disposed.
    pub fn handle(&self, request: JsValue) -> Result<Promise, JsError> {
        let mut opened = self.opened.borrow_mut();
        let opened = opened.as_mut().ok_or_else(|| JsError::new(DISPOSED))?;
        Ok(match read(&request) {
            Ok(request) => Promise::resolve(&response(&opened.answer(request))),
            Err(why) => Promise::reject(&JsError::new(&why).into()),
        })
    }

    /// Empties every store and loads `world`, where given, as `POST /api/reset` with `world` as
    /// its JSON body does: a `Promise` of the answer `handle` would give that request.
    ///
    /// # Errors
    /// The instance was disposed, or `world` cannot be written as JSON.
    pub fn reset(&self, world: JsValue) -> Result<Promise, JsError> {
        let body = if world.is_undefined() || world.is_null() {
            Vec::new()
        } else {
            js_sys::JSON::stringify(&world)
                .map_err(|_| JsError::new("the world cannot be written as JSON"))?
                .as_string()
                .unwrap_or_default()
                .into_bytes()
        };
        let mut opened = self.opened.borrow_mut();
        let opened = opened.as_mut().ok_or_else(|| JsError::new(DISPOSED))?;
        let answered = opened.answer(Request {
            method: "POST".to_owned(),
            path: "/api/reset".to_owned(),
            query: String::new(),
            content_type: Some("application/json".to_owned()),
            body,
        });
        Ok(Promise::resolve(&response(&answered)))
    }

    /// Drops the host and its stores; every later call throws. `free()` releases the instance's
    /// memory as well.
    ///
    /// # Errors
    /// The instance was disposed already.
    pub fn dispose(&self) -> Result<(), JsError> {
        self.opened
            .borrow_mut()
            .take()
            .map(drop)
            .ok_or_else(|| JsError::new(DISPOSED))
    }
}

fn created(config: &JsValue) -> Result<Spindle, String> {
    let text = match config.as_string() {
        Some(text) => text,
        None => js_sys::JSON::stringify(config)
            .ok()
            .and_then(|text| text.as_string())
            .ok_or("the configuration: neither JSON text nor an object JSON can write")?,
    };
    let config: Config =
        serde_json::from_str(&text).map_err(|error| format!("the configuration: {error}"))?;
    let admitted = spindle_embed::admit(config.sources, &config.environment)?;
    let mut reached = Vec::new();
    for placed in admitted.placed() {
        if let Some(state) = placed
            .state
            .filter(|state| state.implementation != "memory")
        {
            return Err(format!(
                "{} has {} bound to `{}`; the Node export keeps state in `memory` only",
                placed.component, state.capability, state.implementation
            ));
        }
        match placed.network {
            None => {}
            Some(NODE) => reached.push(placed.component.to_owned()),
            Some(other) => {
                return Err(format!(
                    "{} has ingress.network bound to `{other}`; the Node export serves `{NODE}` \
                     only",
                    placed.component
                ));
            }
        }
    }
    let opened = admitted.open(Opening {
        stores: Stores::new(),
        reached: reached.clone(),
        decisions: config.decisions,
        answer_default: config.answer_default,
        binding: config.binding,
        worlds: true,
        clock: Arc::new(SystemClock),
    })?;
    let report = opened.surface().report();
    if opened.surface().serves_nothing() {
        let mut lines = report;
        lines.push(Surface::serves_nothing_reason(&reached));
        return Err(lines.join("\n"));
    }
    Ok(Spindle {
        opened: RefCell::new(Some(opened)),
        report,
    })
}

/// `request` as the router takes it.
fn read(request: &JsValue) -> Result<Request, String> {
    let field = |name: &str| Reflect::get(request, &name.into()).unwrap_or(JsValue::UNDEFINED);
    let method = field("method")
        .as_string()
        .ok_or("the request has no method")?;
    let target = field("path").as_string().ok_or("the request has no path")?;
    let (path, carried) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), query.to_owned()),
        None => (target, String::new()),
    };
    let query = field("query").as_string().unwrap_or(carried);
    let body = field("body");
    let body = if let Some(text) = body.as_string() {
        text.into_bytes()
    } else if let Some(bytes) = body.dyn_ref::<Uint8Array>() {
        bytes.to_vec()
    } else if body.is_undefined() || body.is_null() {
        Vec::new()
    } else {
        return Err("the request's body is neither a string nor a Uint8Array".to_owned());
    };
    Ok(Request {
        method,
        path,
        query,
        content_type: content_type(&field("headers")),
        body,
    })
}

/// The `content-type` among `headers`, a plain object or an array of `[name, value]`, its name
/// in any case.
fn content_type(headers: &JsValue) -> Option<String> {
    let pairs: Vec<(String, JsValue)> = if Array::is_array(headers) {
        Array::from(headers)
            .iter()
            .filter_map(|pair| {
                let pair = Array::from(&pair);
                Some((pair.get(0).as_string()?, pair.get(1)))
            })
            .collect()
    } else if headers.is_object() {
        Object::entries(headers.unchecked_ref())
            .iter()
            .filter_map(|pair| {
                let pair = Array::from(&pair);
                Some((pair.get(0).as_string()?, pair.get(1)))
            })
            .collect()
    } else {
        Vec::new()
    };
    pairs
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .and_then(|(_, value)| value.as_string())
}

/// `{ status, headers, body }` for `response`.
fn response(response: &spindle_ingress::Response) -> JsValue {
    let answered = spindle_embed::answered(response);
    let headers = Array::new();
    for (name, value) in &answered.headers {
        headers.push(&Array::of2(&name.into(), &value.into()));
    }
    let body = Uint8Array::from(answered.body.unwrap_or_default().as_bytes());
    let object = Object::new();
    let _ = Reflect::set(&object, &"status".into(), &response.status.into());
    let _ = Reflect::set(&object, &"headers".into(), &headers);
    let _ = Reflect::set(&object, &"body".into(), &body);
    object.into()
}
