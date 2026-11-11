//! The integration tests of `spindle-host`, one module per former `tests/*.rs` file, linked into
//! one test binary (`story:one-test-binary-per-crate`). Test names are unchanged under their
//! module path.
//!
//! `wasm` and `adversary_wasm` compile only for `wasm32-unknown-unknown`; every other module
//! only for native targets, so `--test spindle-host` runs the wasm cases there alone.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_any_filter;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_console;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_links;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_refusal;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_retry;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_rows;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_view_filters;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod adversary_views;
mod adversary_wasm;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod any_filter;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod concurrent_reads;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod console;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod frame;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod generated;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod grants;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod list_append;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod mint;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod native;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod rally;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod refusal;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod retries;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod retry;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod rows;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod supplied;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod telemetry;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod views;
mod wasm;
