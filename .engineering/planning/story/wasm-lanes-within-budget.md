---
format: aep.planning-md/3
id: story:wasm-lanes-within-budget
kind: story
status: active
title: The wasm lanes of the gate end within the step budget
relations:
- serves: vision:spindle
- depends_on: story:test-step-within-budget
scope:
- confidence: inferred
  path: .github/workflows/ci.yml
- confidence: inferred
  path: Cargo.toml
- confidence: cited
  path: Taskfile.yml
- confidence: inferred
  path: scripts
revision: 13
transitions:
- {from: "draft", to: "proposed", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 12, decided_on: {"recorded":{"review_outcome":19}}}
- {from: "proposed", to: "active", at: "2026-11-26T04:51:42Z", actor: "human:operator", revision: 13, decided_on: {"recorded":{"review_outcome":19}}}
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

- `check:node_serves_a_specification` ends within 120 s on an empty `target/`, with the host's
  shared compile cache as it is, measured with `time` in the change's report beside the `slot
  status` header line of that moment, and its `node --test` tests pass unchanged.
- `check:the_static_bundle_builds` ends within 120 s on an empty `target/`, measured the same way,
  and `web/dist` builds as today.
- `check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store`,
  `check:the_browser_store_keeps_state_in_indexeddb` and `check:a_service_worker_serves_a_specification`
  each end within 120 s on an empty `target/`, measured the same way.
- Each of the six lanes runs under `scripts/within.sh 120` (from `story:test-step-within-budget`),
  in `task check` and in CI's `Gate`, and so prints its wall time and fails over 120 s.
- The three `wasm-bindgen-test-runner` lanes set `WASM_BINDGEN_TEST_TIMEOUT` to 60 or below, where
  it is `'120'` today (`Taskfile.yml:58`, `:66`, `:74`). The Node lane keeps
  `node --test --test-timeout=60000` (`Taskfile.yml:83`).
- No wasm lane needs `slot --timeout` to finish within the pool's 10-minute limit.

## Scope

Derived 2026-11-26 by `story-scoper`. Every line is **cited** (read from the story or the tree) or
**inferred** (a reading that could be wrong).

- **Primary surface:** `Taskfile.yml`, the six wasm lanes and the `check` task that calls them (`Taskfile.yml:24-29`, `:46-96`) — cited
- **Files:** `Taskfile.yml:81` (`check:node_serves_a_specification`, `cargo build --release`) and `Taskfile.yml:93` (`web`, `cargo build --release -p spindle-wasm`, reached from `check:the_static_bundle_builds`) — cited
- **Symbols:** tasks `check:node_serves_a_specification`, `check:the_wasm_build_of_the_host_succeeds`, `check:a_command_runs_in_wasm_on_the_memory_store`, `check:the_browser_store_keeps_state_in_indexeddb`, `check:a_service_worker_serves_a_specification`, `check:the_static_bundle_builds`, `web` — cited
- **Also likely:** `Cargo.toml`, a new `[profile.*]` for the gate (the file has none today) — inferred
- **Also likely:** `.github/workflows/ci.yml`, job `check` (`Gate`), only if CI needs its own timing step — inferred
- **Also likely:** `scripts/`, a POSIX sh timer that prints a lane's wall time and fails over 120 s — inferred
- **Documents:** none; `README.md:165` and `website/docs/concepts/environment-and-admission.md:249` show the release build and need no change if the gate's profile is separate — inferred
- **Confidence:** medium. The lanes and their release builds are read; which fix is taken decides the second file.
- **Would collide with:** any unit touching the `check` task or the wasm lanes in `Taskfile.yml`, a `[profile.*]` in `Cargo.toml`, or the `Gate` job of `.github/workflows/ci.yml`
- **Safety:** the Node tests load the module from `target/spindle-node` or `SPINDLE_NODE_MODULE`, not from the cargo profile directory (`crates/spindle-node/tests/node/load.mjs:2`), so a different profile changes only `Taskfile.yml:81-82`; `crates/spindle-cli/tests/spindle-cli/conform.rs:619-625` requires the line `- task: check:conformance` in the `check` task as written, so a timer around `check`'s steps must keep it — inferred

## Shared files

- `Taskfile.yml` (task `check`) and `.github/workflows/ci.yml` (job `Gate`): shared with
  `story:planning-only-prs-skip-tests` and `story:test-step-within-budget`, built before this story
  on the same branch; this story uses the timer `scripts/within.sh` that
  `story:test-step-within-budget` adds.
