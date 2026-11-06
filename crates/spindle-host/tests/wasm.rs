//! The host on `wasm32-unknown-unknown`, run by `wasm-bindgen-test` (`task
//! check:a_command_runs_in_wasm_on_the_memory_store`): the specification compiled from sources in
//! memory, every store in memory, identities minted from the browser's random source and the time
//! read from its clock. Native builds compile this file to nothing.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, SystemClock};
use spindle_kernel::Uuid;
use wasm_bindgen_test::wasm_bindgen_test;

const SYSTEM: &str = include_str!("fixtures/notebooks/system.yaml");
const NOTEBOOKS: &str = include_str!("fixtures/notebooks/domains/notebooks.yaml");

/// An environment that binds every store to `memory`, as a browser provides it.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"browser-memory\"\n",
        "os = \"unknown\"\n",
        "arch = \"wasm32\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[wasm_bindgen_test]
fn a_command_runs_in_wasm_on_the_memory_store() {
    let ir = spindle_spec::compile(
        &[
            "system.yaml".to_owned(),
            "domains/notebooks.yaml".to_owned(),
        ],
        &[SYSTEM.to_owned(), NOTEBOOKS.to_owned()],
    )
    .expect("the notebooks fixture compiles in wasm");
    let mut host = Host::new(
        ir,
        &in_memory(),
        Arc::new(SystemClock),
        std::path::Path::new(""),
    )
    .expect("the host admits the notebooks fixture on memory");

    let report = host
        .submit(
            "desk.notebooks.OpenNotebook",
            object(json!({"title": "Field notes"})),
        )
        .expect("OpenNotebook runs");
    assert_eq!(report.ran.len(), 1, "one command ran: {report:?}");
    assert_eq!(report.ran[0].executed.outcome, "opened");

    let published = host.published();
    assert_eq!(published.len(), 1, "one event published: {published:?}");
    let identity = published[0].event.payload["notebook_id"].clone();
    let minted = Uuid::parse_str(identity.as_str().expect("the identity is text"))
        .expect("the minted identity is a UUID");
    assert_eq!(minted.get_version_num(), 4, "minted from the random source");

    let card = host
        .view("desk.notebooks.NotebookCard", &identity)
        .expect("the view reads")
        .expect("the opened notebook has a card");
    assert_eq!(card["notebook_id"], identity);
    assert_eq!(card["title"], json!("Field notes"));
    let opened_at = card["opened_at"]
        .as_str()
        .expect("opened_at is a timestamp");
    let year = opened_at
        .get(..4)
        .and_then(|year| year.parse::<u32>().ok())
        .expect("opened_at starts with its year");
    assert!(
        year >= 2026,
        "opened_at {opened_at} is read from the browser's clock, not the epoch"
    );
}
