---
format: aep.planning-md/3
id: story:one-test-binary-per-crate
kind: story
status: implemented
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
revision: 6
transitions:
- {from: "draft", to: "proposed", at: "2026-11-06T09:14:32Z", actor: "human:timo", revision: 3}
- {from: "proposed", to: "active", at: "2026-11-06T09:14:32Z", actor: "human:timo", revision: 4}
- {from: "active", to: "implemented", at: "2026-11-06T19:57:14Z", actor: "human:timo", revision: 6, decided_on: {"recorded":{"test_result":1}}}
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

## Order

Leads wave 11. `b10x-chef-orchestrator`, 2026-11-06: "Wave 10: at most 4 units (wave rule, chef-orchestrator SKILL.md § Delivery rule 1). Keep kernel-delete-outcomes, refusal-payload-complete, structured-response-values, fixed-caller-per-grant; one-test-binary-per-crate leads wave 11."
