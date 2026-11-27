//! An embedded host reads the clock its opening gives it and steps none
//! (`story:served-clock-steps`): a reset whose world carries `clock` is refused `409` before any
//! store is emptied, and both methods of `/api/clock` answer `409`.

use std::sync::Arc;

use serde_json::{Value, json};
use spindle_host::{ManualClock, Stores};
use spindle_ingress::{Request, Response};

const SOURCES: [(&str, &str); 2] = [
    (
        "system.yaml",
        include_str!("../../../spindle-cli/tests/fixtures/clock-served/system.yaml"),
    ),
    (
        "domains/slot.yaml",
        include_str!("../../../spindle-cli/tests/fixtures/clock-served/domains/slot.yaml"),
    ),
];

const NODE: &str = include_str!("../../../../examples/environments/node.toml");

/// The `clock-served` fixture of `spindle-cli`, opened on the Node environment with worlds
/// allowed.
fn opened() -> spindle_embed::Opened {
    let sources = SOURCES
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    spindle_embed::admit(sources, NODE)
        .and_then(|admitted| {
            admitted.open(spindle_embed::Opening {
                stores: Stores::new(),
                reached: vec!["desk".to_owned()],
                decisions: None,
                answer_default: false,
                binding: None,
                callers: Vec::new(),
                worlds: true,
                clock: Arc::new(ManualClock::new()),
            })
        })
        .unwrap_or_else(|refused| panic!("the clock-served fixture opens: {refused}"))
}

fn request(method: &str, path: &str, body: &str) -> Request {
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    Request {
        method: method.to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        content_type: Some("application/json".to_owned()),
        body: body.as_bytes().to_vec(),
    }
}

fn body(response: &Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|error| panic!("{error}: {:?}", response.body))
}

/// The events `GET /api/events` answers.
fn events(host: &mut spindle_embed::Opened) -> Vec<Value> {
    let response = host.answer(request("GET", "/api/events", ""));
    assert_eq!(response.status, 200, "{response:?}");
    body(&response)["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events: {response:?}"))
        .clone()
}

#[test]
fn an_embedded_host_refuses_a_reset_world_carrying_clock() {
    let mut host = opened();
    let path = host
        .surface()
        .routes()
        .iter()
        .find(|route| route.operation.name() == "ledger.slot.Book")
        .unwrap_or_else(|| panic!("the surface serves ledger.slot.Book"))
        .path
        .clone();
    let booked = host.answer(request(
        "POST",
        &path,
        &json!({"booking_id": "b1", "deadline": "2026-06-01T00:00:00Z"}).to_string(),
    ));
    assert!((200..300).contains(&booked.status), "{booked:?}");
    assert_eq!(events(&mut host).len(), 1);

    let reset = host.answer(request(
        "POST",
        "/api/reset",
        &json!({"clock": "2099-01-01T00:00:00Z"}).to_string(),
    ));
    assert_eq!(reset.status, 409, "{reset:?}");
    assert!(body(&reset)["refused"].is_string(), "{reset:?}");
    let left = events(&mut host);
    assert_eq!(left.len(), 1, "the refused reset emptied nothing: {left:?}");
    assert_eq!(left[0]["payload"]["booking_id"], "b1", "{left:?}");

    for method in ["GET", "POST"] {
        let answered = host.answer(request(
            method,
            "/api/clock",
            &json!({"at": "2099-01-01T00:00:00Z"}).to_string(),
        ));
        assert_eq!(answered.status, 409, "{method} /api/clock: {answered:?}");
    }
}
