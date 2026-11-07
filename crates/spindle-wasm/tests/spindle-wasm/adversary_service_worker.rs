//! Adversarial cases for `story:service-worker-serves-a-specification`: what the Service Worker
//! answers where its path differs from the loopback listener's (the `Response` it builds, the
//! order of requests in flight, a request that fails before it is routed).

use js_sys::Promise;
use serde_json::{Value, json};
use spindle_wasm::Spindle;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::JsValue;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{Request, RequestInit, Response};

const BROWSER: &str = include_str!("../../../../examples/environments/browser.toml");

fn fresh(prefix: &str) -> String {
    format!(
        "adversary-{prefix}-{}-{}",
        js_sys::Date::now(),
        js_sys::Math::random().to_bits()
    )
}

async fn open(database: &str, binding: Option<String>) -> Spindle {
    let config = json!({
        "sources": crate::SOURCES,
        "environment": BROWSER,
        "decisions": crate::DECISIONS,
        "binding": binding,
        "database": database,
    })
    .to_string();
    Spindle::open(config)
        .await
        .map_err(JsValue::from)
        .expect("the Service Worker opens the host")
}

fn request(method: &str, target: &str, body: &str) -> Request {
    let init = RequestInit::new();
    init.set_method(method);
    if !body.is_empty() {
        init.set_body(&JsValue::from_str(body));
    }
    let request = Request::new_with_str_and_init(target, &init).expect("a request");
    if !body.is_empty() {
        request
            .headers()
            .set("content-type", "application/json")
            .expect("a content type");
    }
    request
}

async fn settled(promise: Promise) -> Result<(u16, String), String> {
    let response: Response = JsFuture::from(promise)
        .await
        .map_err(|error| format!("{error:?}"))?
        .dyn_into()
        .map_err(|_| "the answer is not a Response".to_owned())?;
    let text = JsFuture::from(response.text().expect("a body"))
        .await
        .expect("the body reads")
        .as_string()
        .unwrap_or_default();
    Ok((response.status(), text))
}

/// An `ess-http/1` binding ESS admits: `probe.review.Submit` at `POST /submissions`, its
/// `submitted` outcome answered `205` with the envelope (ESS refuses a body only on `204`). The
/// loopback listener writes that status; the browser's `Response` constructor refuses a body on a
/// null-body status (`204`, `205`, `304`).
fn reset_content_binding() -> String {
    format!(
        "type: ess-http/1\n\
         specification:\n  system: probe\n  version: v1\n  source_digest: sha256:{}\n\
         routes:\n  - command: probe.review.Submit\n    method: post\n    path: /submissions\n\
         \x20   outcomes:\n      submitted:\n        status: 205\n",
        crate::ir().source_digest()
    )
}

#[wasm_bindgen_test]
async fn adversary_an_outcome_bound_to_205_is_answered_205() {
    let spindle = open(&fresh("reset-content"), Some(reset_content_binding())).await;
    let answered = settled(spindle.fetch(request(
        "POST",
        "/submissions",
        r#"{"submission_id": "s5", "amount": 5}"#,
    )))
    .await;
    let (status, _) = answered.unwrap_or_else(|error| {
        panic!(
            "the Service Worker answers the 205 outcome the loopback listener answers, and does \
             not reject the fetch: {error}"
        )
    });
    assert_eq!(status, 205);
}

#[wasm_bindgen_test]
async fn adversary_requests_in_flight_together_are_answered_in_arrival_order() {
    let spindle = open(&fresh("in-flight"), None).await;
    let first = spindle.fetch(request(
        "POST",
        "/review/commands/Submit",
        r#"{"submission_id": "a1", "amount": 1}"#,
    ));
    let second = spindle.fetch(request(
        "POST",
        "/review/commands/Submit",
        r#"{"submission_id": "a2", "amount": 2}"#,
    ));
    let read = spindle.fetch(request("GET", "/review/views/Submissions", ""));
    let first = settled(first).await.expect("the first is answered");
    let second = settled(second).await.expect("the second is answered");
    let read = settled(read).await.expect("the read is answered");
    assert_eq!(first.0, 202, "{}", first.1);
    assert_eq!(second.0, 202, "{}", second.1);
    assert_eq!(read.0, 200, "{}", read.1);
    assert_eq!(
        serde_json::from_str::<Value>(&read.1).expect("the rows are JSON"),
        json!({"rows": [
            {"amount": 1, "submission_id": "a1"},
            {"amount": 2, "submission_id": "a2"},
        ]}),
        "the read sent after both writes sees both"
    );
}

#[wasm_bindgen_test]
async fn adversary_a_request_that_fails_before_routing_does_not_stall_the_next() {
    let spindle = open(&fresh("stall"), None).await;
    let used = request(
        "POST",
        "/review/commands/Submit",
        r#"{"submission_id": "u1", "amount": 1}"#,
    );
    JsFuture::from(used.text().expect("the body reads"))
        .await
        .expect("the body is consumed");
    assert!(
        settled(spindle.fetch(used)).await.is_err(),
        "a request whose body was already read cannot be answered"
    );
    let (status, rows) = settled(spindle.fetch(request("GET", "/review/views/Submissions", "")))
        .await
        .expect("the next request is answered");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&rows).expect("the rows are JSON"),
        json!({"rows": []})
    );
}
