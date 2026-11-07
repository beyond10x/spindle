//! The integration tests of `spindle-wasm`, linked into one test binary.
//!
//! `listener` runs on native targets: the loopback listener answers the exchanges recorded in
//! `tests/fixtures/review/exchanges.json`. `service_worker` compiles only for
//! `wasm32-unknown-unknown` and runs inside a Service Worker of a headless browser under
//! `wasm-bindgen-test-runner` (`task check:a_service_worker_serves_a_specification`), where it
//! answers the same requests and must answer them the same way.

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_service_worker);

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod adversary_service_worker;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod listener;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod service_worker;

/// The review fixture's sources, by the name each is compiled under.
const SOURCES: [(&str, &str); 2] = [
    (
        "system.yaml",
        include_str!("../fixtures/review/system.yaml"),
    ),
    (
        "domains/review.yaml",
        include_str!("../fixtures/review/domains/review.yaml"),
    ),
];

/// The decisions file both hosts answer external outcomes from.
const DECISIONS: &str = include_str!("../fixtures/review/decisions.toml");

/// The requests both hosts answer, in order, each with the answer the loopback listener writes.
const EXCHANGES: &str = include_str!("../fixtures/review/exchanges.json");

/// One request and the answer the loopback listener writes for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct Exchange {
    method: String,
    /// The path and query.
    target: String,
    content_type: Option<String>,
    body: String,
    status: u16,
    allow: Option<String>,
    answer: String,
}

fn exchanges() -> Vec<Exchange> {
    serde_json::from_str(EXCHANGES).expect("the recorded exchanges read")
}

fn ir() -> spindle_spec::EssIr {
    let (names, texts): (Vec<String>, Vec<String>) = SOURCES
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .unzip();
    spindle_spec::compile(&names, &texts).expect("the review fixture compiles")
}
