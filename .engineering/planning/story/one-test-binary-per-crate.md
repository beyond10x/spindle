---
format: aep.planning-md/3
id: story:one-test-binary-per-crate
kind: story
status: draft
title: One integration-test binary per crate
relations:
- serves: vision:spindle
scope:
- confidence: cited
  path: crates/spindle-cli/tests
- confidence: cited
  path: crates/spindle-conformance/tests
- confidence: cited
  path: crates/spindle-docs/src/status.rs
- confidence: cited
  path: crates/spindle-host/tests
- confidence: cited
  path: crates/spindle-kernel/tests
revision: 2
---
# One integration-test binary per crate

## Outcome

Each crate's integration tests build into one test binary (`tests/<crate>/main.rs` with one module
per former file) instead of one binary per `tests/*.rs` file. Today the workspace has about 110
integration-test binaries (spindle-cli 32, spindle-kernel 21, spindle-host 20, spindle-conformance
14, spindle-qemu 7, spindle-telemetry 6, others fewer); each links the crate's dependency graph,
and one full build of a tree writes 78 binaries over 10 MB, 8.2 GB in total (measured in
`spindle-w9-wasm/target/debug/deps`, 2026-11-06). The host was disk-bound at that time (32-53%
iowait).

## Acceptance

- Every test that ran before runs after, with the same name under its module path; the count per
  crate is unchanged (`cargo test --workspace` totals before and after, recorded).
- `cargo test -p <crate>` links one integration-test binary per crate that has tests.
- Citations of tests by `file.rs::name` (`crates/spindle-docs/src/status.rs`, design docs, store
  evidence bodies left as written) still resolve: status citations are updated to the new path.
- Tests that read fixtures through `env!("CARGO_MANIFEST_DIR")` or `CARGO_TARGET_TMPDIR` behave as
  before.

## Origin

The operator, 2026-11-06, after a load check: "figure out, if you can do anything about the load
your own session is causing on a constant basis".
