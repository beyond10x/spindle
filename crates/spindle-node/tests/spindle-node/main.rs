//! The native tests of `spindle-node`, linked into one test binary.
//!
//! The Node export is tested under Node (`task check:node_serves_a_specification`, the
//! `tests/node/*.test.mjs` beside this directory). What it must answer is recorded here, natively:
//! `listener` serves the review fixture on the loopback listener with `run`'s world reader and
//! answers the exchanges `tests/fixtures/world-exchanges.json` records.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod listener;
