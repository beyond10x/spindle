//! The integration tests of `spindle-store-browser`, linked into one test binary.
//!
//! `environment` runs on native targets. `adversary`, `contract` and `reload` compile only for
//! `wasm32-unknown-unknown` and run in a headless browser under `wasm-bindgen-test-runner`
//! (`task check:the_browser_store_keeps_state_in_indexeddb`), where IndexedDB exists.

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod adversary;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod contract;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod environment;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod reload;

/// A database name no other case of this run opens.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn fresh(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}",
        js_sys::Date::now(),
        js_sys::Math::random().to_bits()
    )
}
