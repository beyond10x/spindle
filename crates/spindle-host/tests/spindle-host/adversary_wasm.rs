//! Adversary cases for the wasm build of the host and the kernel it carries: the kernel's own
//! entry points on the memory store, the declared refusal of a SQLite binding when the build
//! leaves SQLite out, and a manual clock read from the browser. Run with `task
//! check:a_command_runs_in_wasm_on_the_memory_store`.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, SystemClock};
use spindle_kernel::{MemoryStore, NoExternals};
use wasm_bindgen_test::wasm_bindgen_test;

const SYSTEM: &str = include_str!("../fixtures/notebooks/system.yaml");
const NOTEBOOKS: &str = include_str!("../fixtures/notebooks/domains/notebooks.yaml");

fn environment(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"browser-{store}\"\n",
            "os = \"unknown\"\n",
            "arch = \"wasm32\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "\"identity.mint\" = [\"uuid-v4\"]\n",
            "clock = [\"system\"]\n",
            "timer = [\"tokio\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

fn compiled() -> ess_compiler::EssIr {
    spindle_spec::compile(
        &[
            "system.yaml".to_owned(),
            "domains/notebooks.yaml".to_owned(),
        ],
        &[SYSTEM.to_owned(), NOTEBOOKS.to_owned()],
    )
    .expect("the notebooks fixture compiles in wasm")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// `spindle_kernel::execute` is the kernel's documented entry point and the story builds the
/// kernel for wasm32: a command that stamps a generated `Timestamp` must run there on the memory
/// store, its time read from the browser, not abort on `std::time::SystemTime::now`.
#[wasm_bindgen_test]
fn the_kernel_execute_entry_point_runs_a_timestamped_command_in_wasm() {
    let ir = compiled();
    let mut store = MemoryStore::default();
    let executed = spindle_kernel::execute(
        &ir,
        &mut store,
        &mut NoExternals,
        "desk.notebooks.OpenNotebook",
        &object(json!({"title": "Field notes"})),
    )
    .expect("OpenNotebook runs through spindle_kernel::execute");
    assert_eq!(executed.outcome, "opened");
}

/// A build without the `sqlite` feature refuses an environment that binds a store to `sqlite`
/// with a declared `HostError::Unsupported`, not a panic or a silent memory store.
#[wasm_bindgen_test]
fn a_sqlite_binding_is_refused_as_unsupported_in_wasm() {
    let refused = Host::new(
        compiled(),
        &environment("sqlite"),
        Arc::new(SystemClock),
        std::path::Path::new(""),
    );
    match refused {
        Err(HostError::Unsupported(reason)) => {
            assert!(
                reason.contains("sqlite"),
                "the refusal names sqlite: {reason}"
            );
        }
        Err(other) => panic!("expected Unsupported, got {other}"),
        Ok(_) => panic!("a sqlite binding was admitted by a build without SQLite"),
    }
}

/// `ManualClock::new` reads the browser's civil time in wasm, as the clock module documents.
#[wasm_bindgen_test]
fn a_manual_clock_opened_in_wasm_stamps_the_current_year() {
    let mut host = Host::new(
        compiled(),
        &environment("memory"),
        Arc::new(ManualClock::new()),
        std::path::Path::new(""),
    )
    .expect("the host admits the notebooks fixture on memory");
    host.submit(
        "desk.notebooks.OpenNotebook",
        object(json!({"title": "Field notes"})),
    )
    .expect("OpenNotebook runs");
    let identity =
        host.published().expect("the events read")[0].event.payload["notebook_id"].clone();
    let card = host
        .view("desk.notebooks.NotebookCard", &identity)
        .expect("the view reads")
        .expect("the opened notebook has a card");
    let opened_at = card["opened_at"].as_str().expect("opened_at is text");
    let year = opened_at
        .get(..4)
        .and_then(|year| year.parse::<u32>().ok())
        .expect("opened_at starts with its year");
    assert!(year >= 2026, "opened_at {opened_at} read from the browser");
}
