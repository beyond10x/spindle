//! The native tests of `spindle-node`, linked into one test binary.
//!
//! The Node export is tested under Node (`task check:node_serves_a_specification`, the
//! `tests/node/*.test.mjs` beside this directory). What it must answer is recorded here, natively:
//! `listener` serves the review fixture on the loopback listener with `run`'s world reader and
//! answers the exchanges `tests/fixtures/world-exchanges.json` records; `fixed_actor` serves the
//! fixed-callers fixture with two fixed actors and answers
//! `tests/fixtures/fixed-actor-exchanges.json`, and checks that the opening refuses a fixed actor
//! as `run` does.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod fixed_actor;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod listener;
