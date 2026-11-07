//! A Service Worker answers a specification's routes and the console routes from the wasm host
//! (`story:service-worker-serves-a-specification`): these cases run inside a Service Worker of a
//! headless browser, the specification compiled there, its state in IndexedDB
//! (`examples/environments/browser.toml`), its external outcomes answered from a decisions file.

use std::cell::Cell;
use std::rc::Rc;

use js_sys::{Function, Promise, Reflect};
use serde_json::{Value, json};
use spindle_wasm::Spindle;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Request, RequestInit, Response};

use crate::Exchange;

const BROWSER: &str = include_str!("../../../../examples/environments/browser.toml");

/// A database name no other case of this run opens.
fn fresh(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        js_sys::Date::now(),
        js_sys::Math::random().to_bits()
    )
}

/// The configuration a Service Worker opens the host with: the review fixture, the browser
/// environment and `decisions`.
fn config(database: &str, decisions: &str) -> String {
    json!({
        "sources": crate::SOURCES,
        "environment": BROWSER,
        "decisions": decisions,
        "database": database,
    })
    .to_string()
}

async fn open(database: &str, decisions: &str) -> Spindle {
    Spindle::open(config(database, decisions))
        .await
        .map_err(JsValue::from)
        .expect("the Service Worker opens the host")
}

fn request(method: &str, target: &str, content_type: Option<&str>, body: &str) -> Request {
    let init = RequestInit::new();
    init.set_method(method);
    if !body.is_empty() {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(target, &init).expect("a request");
    if let Some(content_type) = content_type {
        request
            .headers()
            .set("content-type", content_type)
            .expect("a content type");
    }
    request
}

/// What the host answers `request`: the status, the `Allow` header and the body text.
async fn answer(spindle: &Spindle, request: Request) -> (u16, Option<String>, String) {
    let response: Response = JsFuture::from(spindle.fetch(request))
        .await
        .expect("the host answers")
        .dyn_into()
        .expect("the answer is a Response");
    let allow = response.headers().get("allow").expect("headers read");
    let text = JsFuture::from(response.text().expect("a body"))
        .await
        .expect("the body reads")
        .as_string()
        .expect("the body is text");
    (response.status(), allow, text)
}

/// Every request the global scope sends to the network while it is installed, by count: the
/// scope's `fetch` is replaced by one that counts and refuses.
struct Egress {
    sent: Rc<Cell<u32>>,
    original: JsValue,
    _counting: Closure<dyn FnMut(JsValue) -> Promise>,
}

impl Egress {
    fn watch() -> Self {
        let global = js_sys::global();
        let original = Reflect::get(&global, &"fetch".into()).expect("the scope has fetch");
        let sent = Rc::new(Cell::new(0));
        let counter = sent.clone();
        let counting = Closure::<dyn FnMut(JsValue) -> Promise>::new(move |_: JsValue| {
            counter.set(counter.get() + 1);
            Promise::reject(&JsValue::from_str("the network is closed to this test"))
        });
        Reflect::set(
            &global,
            &"fetch".into(),
            counting.as_ref().unchecked_ref::<Function>(),
        )
        .expect("fetch is replaced");
        Self {
            sent,
            original,
            _counting: counting,
        }
    }

    fn sent(&self) -> u32 {
        self.sent.get()
    }
}

impl Drop for Egress {
    fn drop(&mut self) {
        let _ = Reflect::set(&js_sys::global(), &"fetch".into(), &self.original);
    }
}

/// How many resources the scope's performance timeline holds: every request it sent to the
/// network adds one.
fn resources() -> u32 {
    let performance =
        Reflect::get(&js_sys::global(), &"performance".into()).expect("the scope has performance");
    let entries = Reflect::get(&performance, &"getEntriesByType".into())
        .expect("getEntriesByType")
        .dyn_into::<Function>()
        .expect("getEntriesByType is a function")
        .call1(&performance, &"resource".into())
        .expect("the entries read");
    js_sys::Array::from(&entries).length()
}

#[wasm_bindgen_test]
async fn a_fetch_to_a_bound_route_is_answered_in_the_browser() {
    let spindle = open(&fresh("bound"), crate::DECISIONS).await;
    let before = resources();
    let egress = Egress::watch();

    let recorded = crate::exchanges();
    let mut answered = Vec::new();
    for exchange in &recorded {
        let asked = request(
            &exchange.method,
            &exchange.target,
            exchange.content_type.as_deref(),
            &exchange.body,
        );
        let (status, allow, answer) = answer(&spindle, asked).await;
        answered.push(Exchange {
            status,
            allow,
            answer,
            ..exchange.clone()
        });
    }
    let (status, _, model) = answer(&spindle, request("GET", "/api/model", None, "")).await;

    assert_eq!(egress.sent(), 0, "no request left the Service Worker");
    drop(egress);
    assert_eq!(
        resources(),
        before,
        "the performance timeline holds no request sent while the host answered"
    );
    assert_eq!(
        answered,
        recorded,
        "the Service Worker answers what the loopback listener answers; it wrote:\n{}",
        serde_json::to_string_pretty(&answered).unwrap_or_default()
    );
    assert_eq!(status, 200);
    assert_eq!(
        model,
        spindle_ingress::console::model(&crate::ir()),
        "GET /api/model answers the IR compiled in the browser"
    );

    for exchange in &recorded {
        let asked = request(&exchange.method, &exchange.target, None, "");
        assert_eq!(
            spindle.serves(&asked),
            exchange.status != 404,
            "{} {}: the Service Worker answers a request the surface routes and leaves the rest \
             to the network",
            exchange.method,
            exchange.target
        );
    }
}

#[wasm_bindgen_test]
async fn an_external_outcome_is_answered_from_the_decisions_file() {
    let submit = r#"{"submission_id": "s7", "amount": 70}"#;
    let json = Some("application/json");

    let flagging = open(&fresh("flagging"), "flagged = true\nescalated = false\n").await;
    let (_, _, flagged) = answer(
        &flagging,
        request("POST", "/review/commands/Submit", json, submit),
    )
    .await;
    let flagged: Value = serde_json::from_str(&flagged).expect("the answer is JSON");
    assert_eq!(
        flagged["outcome"],
        json!("flagged"),
        "the decisions file says the fraud desk flags it: {flagged}"
    );
    let (_, _, rows) = answer(
        &flagging,
        request("GET", "/review/views/Submissions", None, ""),
    )
    .await;
    assert_eq!(
        serde_json::from_str::<Value>(&rows).expect("the rows are JSON"),
        json!({"rows": []}),
        "a flagged submission is not kept"
    );

    let passing = open(&fresh("passing"), "flagged = false\nescalated = false\n").await;
    let (_, _, submitted) = answer(
        &passing,
        request("POST", "/review/commands/Submit", json, submit),
    )
    .await;
    let submitted: Value = serde_json::from_str(&submitted).expect("the answer is JSON");
    assert_eq!(
        submitted["outcome"],
        json!("submitted"),
        "the decisions file lets the submission through: {submitted}"
    );

    let silent = open(&fresh("silent"), "flagged = false\n").await;
    let (status, _, refused) = answer(
        &silent,
        request("POST", "/review/commands/Submit", json, submit),
    )
    .await;
    assert_ne!(
        status, 202,
        "an outcome the decisions file does not answer is not taken: {refused}"
    );
    assert!(
        refused.contains("escalated"),
        "the refusal names the unanswered outcome: {refused}"
    );
}
