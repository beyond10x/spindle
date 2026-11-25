---
format: aep.planning-md/3
id: story:wasm-lanes-within-budget
kind: story
status: draft
title: The wasm lanes of the gate end within the step budget
relations:
- serves: vision:spindle
revision: 1
---
# The wasm lanes of the gate end within the step budget

## Outcome

Each wasm lane of `task check` ends within 120 s of wall time on a cold cache, and no test in
them runs longer than 60 s: `check:node_serves_a_specification`,
`check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store`,
`check:the_browser_store_keeps_state_in_indexeddb`, `check:a_service_worker_serves_a_specification`
and `check:the_static_bundle_builds`.

Measured 2026-11-25 on the workstation pool, branch feat/node-export-fixed-actor:
`check:node_serves_a_specification` ran 626 s, of which `node --test` took 0.28 s (23 tests)
and the cold `cargo build --release --target wasm32-unknown-unknown -p spindle-node` about 624 s.
Without `slot --timeout 30m` the pool's 10-minute limit ends the lane.

The cost is a compile, so the fix is in how the lanes build: one wasm build shared by the lanes
that need the same artifact, a build profile for the gate that is not the release profile, or a
committed artifact with a digest of its sources, as the repository rules allow.

## Acceptance

- `check:node_serves_a_specification` ends within 120 s on a cold `target/`, measured with `time`
  in the change's report, and its tests pass unchanged.
- Every wasm lane listed above prints its wall time and fails over 120 s.

## Scope

- `Taskfile.yml`, `Cargo.toml` profiles, `.github/workflows/ci.yml` (inferred).
